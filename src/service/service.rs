use anyhow::Context;
use anyhow::Result;
#[cfg(unix)]
use libc::geteuid;
#[cfg(unix)]
use libc::getrlimit;
#[cfg(unix)]
use libc::rlimit;
#[cfg(unix)]
use libc::setpgid;
#[cfg(unix)]
use libc::setrlimit;
#[cfg(unix)]
use libc::signal;
#[cfg(unix)]
use libc::RLIMIT_NOFILE;
#[cfg(unix)]
use libc::SIGPIPE;
#[cfg(unix)]
use libc::SIG_IGN;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::env;
use std::fs::OpenOptions;
#[cfg(unix)]
use std::io::BufRead;
#[cfg(unix)]
use std::io::BufReader;
#[cfg(unix)]
use std::io::BufWriter;
#[cfg(not(unix))]
use std::io::Read;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::os::unix::net::UnixListener;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Once;
use std::thread;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
#[cfg(not(unix))]
use tiny_http::Header;
#[cfg(not(unix))]
use tiny_http::Method;
#[cfg(not(unix))]
use tiny_http::Response;
#[cfg(not(unix))]
use tiny_http::Server;
#[cfg(not(unix))]
use tiny_http::StatusCode;

use crate::channels;
use crate::communication::adapters as communication_adapters;
use crate::context_health;
use crate::execution::agent::direct_session::TerminalBenchPreflightSpec;
use crate::governance;
use crate::inference::runtime_control;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::supervisor;
use crate::inference::turn_loop;
use crate::lcm;
use crate::mission::plan;
use crate::mission::tickets;
use crate::review;
use crate::schedule;
use crate::scrape;
use crate::secrets;
use crate::service::core_state_machine::{
    ArtifactKind, ArtifactRef, CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreState,
    CoreTransitionRequest, RuntimeLane,
};
use crate::service::core_transition_guard::enforce_core_transition;
use crate::state_invariants;
use crate::verification;

#[cfg(not(unix))]
const DEFAULT_SERVICE_HOST: &str = "127.0.0.1";
#[cfg(not(unix))]
const DEFAULT_SERVICE_PORT: &str = "12435";
const SERVICE_PID_RELATIVE_PATH: &str = "runtime/ctox_service.pid";
const SERVICE_LOG_RELATIVE_PATH: &str = "runtime/ctox_service.log";
const SERVICE_SOCKET_RELATIVE_PATH: &str = "runtime/ctox_service.sock";
const SYSTEMD_USER_UNIT_NAME: &str = "ctox.service";
const CHANNEL_ROUTER_POLL_SECS: u64 = 8;
const CHANNEL_SYNC_POLL_SECS: u64 = 60;
const MISSION_MAINTENANCE_POLL_SECS: u64 = 15;
const HARNESS_AUDIT_TICK_SECS: u64 = 300;
const CHANNEL_ROUTER_LEASE_OWNER: &str = "ctox-service";
const QUEUE_PRESSURE_GUARD_THRESHOLD: usize = 20;
const QUEUE_GUARD_SOURCE_LABEL: &str = "queue-guard";
const PLATFORM_EXPERTISE_KIND: &str = "platform-expertise-pass";
const PLATFORM_IMPLEMENTATION_KIND: &str = "platform-implementation";
const STRATEGIC_DIRECTION_KIND: &str = "strategic-direction-pass";
const FOUNDER_COMMUNICATION_REWORK_KIND: &str = "founder-communication-rework";
const RUNTIME_API_RETRY_KIND: &str = "runtime-api-retry";
const FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD: usize = 2;
const REVIEW_CHECKPOINT_REQUEUE_BLOCK_THRESHOLD: usize = 2;
const MAX_REVIEW_CHECKPOINT_REQUEUE_BLOCK_THRESHOLD: usize = 10;
const SERVICE_SHUTDOWN_TIMEOUT_SECS: u64 = 15;
const SERVICE_SHUTDOWN_POLL_MILLIS: u64 = 150;
const SYSTEMCTL_USER_TIMEOUT_SECS: u64 = 5;
const CTO_DRIFT_KIND: &str = "cto-drift-correction";

static SERVICE_PANIC_HOOK: Once = Once::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub running: bool,
    pub busy: bool,
    pub pid: Option<u32>,
    pub listen_addr: String,
    pub autostart_enabled: bool,
    pub manager: String,
    pub pending_count: usize,
    #[serde(default)]
    pub pending_previews: Vec<String>,
    #[serde(default)]
    pub current_goal_preview: Option<String>,
    pub active_source_label: Option<String>,
    pub recent_events: Vec<String>,
    pub last_error: Option<String>,
    pub last_completed_at: Option<String>,
    pub last_reply_chars: Option<usize>,
    pub monitor_last_check_at: Option<String>,
    pub monitor_alerts: Vec<String>,
    pub monitor_last_error: Option<String>,
    /// F3: the structured outcome of the most recent agent assistant turn
    /// for the chat conversation. `None` when there is no assistant row yet
    /// or when the row predates the schema upgrade.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent_outcome: Option<String>,
    #[serde(default)]
    pub work_hours: crate::service::working_hours::WorkHoursSnapshot,
}

#[cfg(any(test, not(unix)))]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct ServiceStatusWire {
    running: bool,
    busy: bool,
    pid: Option<u32>,
    listen_addr: String,
    autostart_enabled: bool,
    manager: String,
    pending_count: usize,
    pending_previews: Vec<String>,
    current_goal_preview: Option<String>,
    active_source_label: Option<String>,
    recent_events: Vec<String>,
    last_error: Option<String>,
    last_completed_at: Option<String>,
    last_reply_chars: Option<usize>,
    monitor_last_check_at: Option<String>,
    monitor_alerts: Vec<String>,
    monitor_last_error: Option<String>,
    last_agent_outcome: Option<String>,
    work_hours: crate::service::working_hours::WorkHoursSnapshot,
}

impl ServiceStatus {
    fn stopped(root: &Path) -> Self {
        let systemd = systemd_unit_status(root).ok().flatten();
        Self {
            running: false,
            busy: false,
            pid: read_pid_file(root),
            listen_addr: service_listen_addr(root),
            autostart_enabled: systemd
                .as_ref()
                .map(|status| status.enabled)
                .unwrap_or(false),
            manager: systemd
                .map(|_| "systemd-user".to_string())
                .unwrap_or_else(|| "process".to_string()),
            pending_count: 0,
            pending_previews: Vec::new(),
            current_goal_preview: None,
            active_source_label: None,
            recent_events: Vec::new(),
            last_error: None,
            last_completed_at: None,
            last_reply_chars: None,
            monitor_last_check_at: None,
            monitor_alerts: Vec::new(),
            monitor_last_error: None,
            last_agent_outcome: None,
            work_hours: crate::service::working_hours::snapshot(root),
        }
    }
}

#[cfg(any(test, not(unix)))]
fn parse_service_status(body: &str, root: &Path) -> Result<ServiceStatus> {
    let wire: ServiceStatusWire =
        serde_json::from_str(body).context("failed to parse CTOX service status")?;
    Ok(ServiceStatus {
        running: wire.running,
        busy: wire.busy,
        pid: wire.pid,
        listen_addr: if wire.listen_addr.trim().is_empty() {
            service_listen_addr(root)
        } else {
            wire.listen_addr
        },
        autostart_enabled: wire.autostart_enabled,
        manager: if wire.manager.trim().is_empty() {
            "process".to_string()
        } else {
            wire.manager
        },
        pending_count: wire.pending_count,
        pending_previews: wire.pending_previews,
        current_goal_preview: wire.current_goal_preview,
        active_source_label: wire.active_source_label,
        recent_events: wire.recent_events,
        last_error: wire.last_error,
        last_completed_at: wire.last_completed_at,
        last_reply_chars: wire.last_reply_chars,
        monitor_last_check_at: wire.monitor_last_check_at,
        monitor_alerts: wire.monitor_alerts,
        monitor_last_error: wire.monitor_last_error,
        last_agent_outcome: wire.last_agent_outcome,
        work_hours: wire.work_hours,
    })
}

#[cfg(not(unix))]
#[derive(Debug, Serialize, Deserialize)]
struct ChatSubmitRequest {
    prompt: String,
    #[serde(default)]
    thread_key: Option<String>,
    #[serde(default)]
    outbound_email: Option<channels::FounderOutboundAction>,
    /// Operator-set anchor for TUI-initiated proactive outbound jobs that
    /// have no leased inbound message key. Routed through into
    /// `QueuedPrompt.outbound_anchor` verbatim.
    #[serde(default)]
    outbound_anchor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AcceptedResponse {
    accepted: bool,
    status: String,
}

#[derive(Debug, Clone)]
pub struct PreparedChatPrompt {
    pub prompt: String,
    pub auto_ingested_secrets: usize,
    pub suggested_skill: Option<String>,
}

#[cfg(unix)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ServiceIpcRequest {
    Status,
    ChatSubmit {
        prompt: String,
        #[serde(default)]
        thread_key: Option<String>,
        #[serde(default)]
        outbound_email: Option<channels::FounderOutboundAction>,
        /// Operator-set anchor for TUI-initiated proactive outbound jobs
        /// that have no leased inbound message key. Routed through into
        /// `QueuedPrompt.outbound_anchor` verbatim.
        #[serde(default)]
        outbound_anchor: Option<String>,
    },
    Stop,
    ScrapeApi {
        path: String,
    },
}

#[cfg(unix)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ServiceIpcResponse {
    Status(ServiceStatus),
    Accepted(AcceptedResponse),
    Json {
        status: u16,
        payload: serde_json::Value,
    },
    Error {
        message: String,
    },
}

#[derive(Debug)]
struct SharedState {
    busy: bool,
    pending_prompts: VecDeque<QueuedPrompt>,
    leased_message_keys_inflight: HashSet<String>,
    current_goal_preview: Option<String>,
    active_source_label: Option<String>,
    recent_events: VecDeque<String>,
    last_error: Option<String>,
    last_completed_at: Option<String>,
    last_reply_chars: Option<usize>,
    last_progress_epoch_secs: u64,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            busy: false,
            pending_prompts: VecDeque::new(),
            leased_message_keys_inflight: HashSet::new(),
            current_goal_preview: None,
            active_source_label: None,
            recent_events: VecDeque::new(),
            last_error: None,
            last_completed_at: None,
            last_reply_chars: None,
            last_progress_epoch_secs: current_epoch_secs(),
        }
    }
}

/// A prompt enqueued for the agent to work on.
///
/// `outbound_email` carries explicit operator intent that this job is an
/// owner/founder/admin-targeted outbound email. When set, the post-turn
/// review pipeline can approve the draft and the outcome gate can require an
/// accepted outbound artifact. The service does not send the mail for the
/// agent; the active agent run must execute the reviewed send command itself
/// after approval. The field is the *only* signal used for that routing —
/// there is no text-scraping or keyword-based fallback in core. Recipient
/// eligibility is still gated by
/// the deterministic `protected_recipient_policies` check against the
/// configured founder/owner/admin address lists.
#[derive(Debug, Clone)]
struct QueuedPrompt {
    prompt: String,
    goal: String,
    preview: String,
    source_label: String,
    suggested_skill: Option<String>,
    leased_message_keys: Vec<String>,
    leased_ticket_event_keys: Vec<String>,
    thread_key: Option<String>,
    workspace_root: Option<String>,
    ticket_self_work_id: Option<String>,
    outbound_email: Option<channels::FounderOutboundAction>,
    /// Stable anchor key used to dedupe and reference review approvals when
    /// the job has no leased inbound message (e.g. TUI-initiated proactive
    /// outbound). Set explicitly by callers; never inferred at routing time.
    outbound_anchor: Option<String>,
}

#[derive(Debug, Clone)]
struct DurableSelfWorkQueueRequest {
    kind: String,
    title: String,
    prompt: String,
    thread_key: String,
    workspace_root: Option<String>,
    priority: String,
    suggested_skill: Option<String>,
    parent_message_key: Option<String>,
    metadata: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExpertisePassSpec {
    pass_kind: &'static str,
    display_name: &'static str,
    suggested_skill: &'static str,
}

const PLATFORM_EXPERTISE_PASSES: [ExpertisePassSpec; 3] = [
    ExpertisePassSpec {
        pass_kind: "platform-ia",
        display_name: "platform IA",
        suggested_skill: "plan-orchestrator",
    },
    ExpertisePassSpec {
        pass_kind: "messaging-wording",
        display_name: "messaging and wording",
        suggested_skill: "plan-orchestrator",
    },
    ExpertisePassSpec {
        pass_kind: "ui-ux",
        display_name: "UI and UX",
        suggested_skill: "frontend-skill",
    },
];

#[derive(Debug, Clone)]
enum CompletionReviewDisposition {
    None,
    Approved,
    Hold {
        summary: String,
    },
    NoSend {
        summary: String,
    },
    RequeueSelfWork {
        work_id: String,
        summary: String,
    },
    ContinueSelfWork {
        work_id: String,
        summary: String,
    },
    /// Lightweight in-process body fix triggered when every reviewer finding
    /// is structurally tagged `rewrite`. The post-turn handler synthesises a
    /// new `QueuedPrompt` with `source_label = "review-rewrite"` that
    /// re-uses the parent job's outbound metadata and inlines the prior
    /// body. No durable state mutation, no new plan goal, no queue task —
    /// just a fast in-process turn that converges the body.
    RewriteOnly {
        findings: Vec<RewriteFinding>,
        prior_body: String,
        anchor_message_key: Option<String>,
        review_summary: String,
    },
    /// In-process continuation after a substantive review finding. The
    /// reviewer remains a quality gate: it explains what is wrong, but it
    /// does not spawn durable work or perform the task. The same executor
    /// receives the feedback as the next prompt with the original outbound
    /// metadata preserved.
    FeedbackRetry {
        feedback_prompt: String,
        review_summary: String,
    },
}

/// Pure-data finding consumed by the lightweight rewrite path. Mirrors the
/// `category: rewrite` half of `review::CategorizedFinding`; the rework half
/// stays inside `RequeueSelfWork`'s payload as semantic findings strings.
#[derive(Debug, Clone)]
pub(crate) struct RewriteFinding {
    pub id: String,
    pub evidence: String,
    pub corrective_action: String,
}

/// Result of classifying the reviewer's structured findings list. The
/// dispatcher consumes the classification verbatim — no fallback heuristics,
/// no string scraping. Empty findings ⇒ `Approved`; all entries tagged
/// `rewrite` ⇒ `RewriteOnly`; any other mix (rework-only or mixed) ⇒
/// `Substantive`. Missing-category items are coerced to `Rework` upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewRoutingClass {
    Approved,
    RewriteOnly,
    Stale,
    Substantive,
}

fn classify_findings(findings: &[review::CategorizedFinding]) -> ReviewRoutingClass {
    if findings.is_empty() {
        return ReviewRoutingClass::Approved;
    }
    if findings
        .iter()
        .all(|f| f.category == review::FindingCategory::Rewrite)
    {
        ReviewRoutingClass::RewriteOnly
    } else if findings.iter().any(|f| f.category.is_stale()) {
        ReviewRoutingClass::Stale
    } else {
        ReviewRoutingClass::Substantive
    }
}

fn review_outcome_is_terminal_no_send(outcome: &review::ReviewOutcome) -> bool {
    let mut text = outcome.summary.to_ascii_lowercase();
    for value in outcome
        .failed_gates
        .iter()
        .chain(outcome.semantic_findings.iter())
        .chain(outcome.open_items.iter())
        .chain(outcome.evidence.iter())
    {
        text.push('\n');
        text.push_str(&value.to_ascii_lowercase());
    }
    for finding in &outcome.categorized_findings {
        text.push('\n');
        text.push_str(&finding.evidence.to_ascii_lowercase());
        text.push('\n');
        text.push_str(&finding.corrective_action.to_ascii_lowercase());
    }

    let says_no_send = contains_any(
        &text,
        &[
            "no-send",
            "no send",
            "do not send",
            "nicht senden",
            "keine weitere founder-mail",
            "keine weitere mail",
            "no further founder",
            "no founder reply",
            "no immediate founder reply",
            "should not be sent",
            "sollte nicht gesendet",
        ],
    );
    let says_wait = contains_any(
        &text,
        &[
            "wait mode",
            "wait until",
            "warte",
            "warten",
            "until the founders provide",
            "until marco",
            "until michael",
            "until olaf",
            "await",
            "konkrete inputs",
            "technical inputs",
            "crm/tool",
            "sync scope",
        ],
    );
    let says_missing_work = contains_any(
        &text,
        &[
            "missing deliverable",
            "missing required",
            "fehlende fachliche arbeit",
            "must be done before",
            "muss erledigt werden",
            "send a corrected",
            "respond directly",
        ],
    );

    says_no_send && says_wait && !says_missing_work
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

/// Source label applied to lightweight rewrite-only post-turn prompts. Kept
/// distinct from `tui` / `queue` / `plan` / `ticket:local` so the dispatcher
/// can identify them in logs and the pipeline status surface.
const REVIEW_REWRITE_SOURCE_LABEL: &str = "review-rewrite";

/// Default convergence threshold for consecutive rewrite-only iterations.
/// Overridable via the `CTOX_MISSION_REWRITE_FAILURE_THRESHOLD` env var.
const DEFAULT_REWRITE_FAILURE_THRESHOLD: i64 = 1;
const MAX_REWRITE_FAILURE_THRESHOLD: i64 = 10;
const DEFAULT_AGENT_FAILURE_THRESHOLD: i64 = 2;
const MAX_AGENT_FAILURE_THRESHOLD: i64 = 6;

fn completion_review_disposition_label(disposition: &CompletionReviewDisposition) -> &'static str {
    match disposition {
        CompletionReviewDisposition::None => "none",
        CompletionReviewDisposition::Approved => "approved",
        CompletionReviewDisposition::Hold { .. } => "hold",
        CompletionReviewDisposition::NoSend { .. } => "no-send",
        CompletionReviewDisposition::RequeueSelfWork { .. } => "requeue-self-work",
        CompletionReviewDisposition::ContinueSelfWork { .. } => "continue-self-work",
        CompletionReviewDisposition::RewriteOnly { .. } => REVIEW_REWRITE_SOURCE_LABEL,
        CompletionReviewDisposition::FeedbackRetry { .. } => "feedback-retry",
    }
}

fn short_terminal_bench_artifact_reply_disposition(
    root: &Path,
    job: &QueuedPrompt,
    reply_text: &str,
) -> Option<CompletionReviewDisposition> {
    if !is_terminal_bench_controller_artifact_job(job) {
        return None;
    }
    if reply_text.trim().chars().count() > 8 {
        return None;
    }

    let expected = expected_outcome_artifacts_for_job(job);
    if expected.is_empty() {
        return None;
    }
    let delivered = delivered_outcome_artifacts_for_job(root, job, &expected).unwrap_or_default();
    let runtime_refs_ok =
        validate_terminal_bench_controller_runtime_refs(root, job, &expected).is_ok();
    if delivered.len() == expected.len() && runtime_refs_ok {
        return None;
    }

    let missing_count = expected.len().saturating_sub(delivered.len());
    Some(CompletionReviewDisposition::Hold {
        summary: format!(
            "Terminal-Bench controller stayed open because the worker returned only {} character(s) and did not satisfy the durable artifact contract. Missing or stale artifacts: {missing_count}/{}. The harness/review must not create files or queue work; retry the same worker with feedback so it performs the shell work itself.",
            reply_text.chars().count(),
            expected.len()
        ),
    })
}

struct ServiceExitGuard {
    pid: u32,
}

impl Drop for ServiceExitGuard {
    fn drop(&mut self) {
        eprintln!("ctox service exiting pid={}", self.pid);
    }
}

pub fn run_foreground(root: &Path) -> Result<()> {
    if let Some(reason) = crate::service::working_hours::hold_reason(root) {
        eprintln!("ctox service not started: {reason}");
        return Ok(());
    }
    let runtime_dir = root.join("runtime");
    std::fs::create_dir_all(&runtime_dir)
        .with_context(|| format!("failed to create runtime dir {}", runtime_dir.display()))?;
    install_service_panic_hook();
    #[cfg(unix)]
    unsafe {
        signal(SIGPIPE, SIG_IGN);
    }
    let _exit_guard = ServiceExitGuard {
        pid: std::process::id(),
    };
    eprintln!(
        "ctox service boot pid={} root={}",
        std::process::id(),
        root.display()
    );
    let active_level = crate::autonomy::AutonomyLevel::from_root(root);
    eprintln!("ctox service autonomy level: {active_level}");
    channels::ensure_store(root)?;
    governance::ensure_governance(root)?;
    if let Err(err) = crate::skill_store::bootstrap_embedded_system_skills(root) {
        eprintln!("ctox service: bootstrap_embedded_system_skills failed: {err:#}");
    }
    let db_path = crate::paths::core_db(&root);
    let _ = crate::lcm::LcmEngine::open(&db_path, crate::lcm::LcmConfig::default())?;
    let listen_addr = service_listen_addr(root);
    write_pid_file(root, std::process::id())?;
    let state = Arc::new(Mutex::new(SharedState::default()));
    run_boot_state_invariant_check(root, &state);
    run_boot_auto_submitted_reclassifier(root, &state);
    release_stale_service_communication_leases_on_boot(root, &state);
    push_event(&state, format!("Loop ready on {}", listen_addr));
    start_channel_router(root.to_path_buf(), state.clone());
    start_channel_syncer(root.to_path_buf());
    start_mission_maintenance_loop(root.to_path_buf(), state.clone());
    start_harness_audit_watcher(root.to_path_buf(), state.clone());
    start_work_hours_dispatcher(root.to_path_buf(), state.clone());
    // Keep the service control plane idle-cheap. Managed runtimes are started
    // on demand by agent turns; boot-time prewarm is opt-in because a local
    // model supervisor can consume CPU even when there is no queued work.
    if runtime_env::config_flag(root, "CTOX_SERVICE_PREWARM_BACKENDS") {
        supervisor::start_backend_supervisor(root.to_path_buf());
    }
    #[cfg(unix)]
    let socket_path = service_socket_path(root);
    let mut announced_ready = false;
    loop {
        #[cfg(unix)]
        let bind_result = {
            let _ = std::fs::remove_file(&socket_path);
            UnixListener::bind(&socket_path)
        };
        #[cfg(not(unix))]
        let server = match Server::http(&listen_addr) {
            Ok(server) => server,
            Err(err) => {
                eprintln!("ctox service bind error on {listen_addr}: {err}");
                thread::sleep(Duration::from_millis(250));
                continue;
            }
        };
        #[cfg(unix)]
        let listener = match bind_result {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!(
                    "ctox service bind error on {}: {err}",
                    socket_path.display()
                );
                thread::sleep(Duration::from_millis(250));
                continue;
            }
        };
        if !announced_ready {
            eprintln!("ctox service listening on {listen_addr}");
            announced_ready = true;
        } else {
            eprintln!("ctox service re-bound on {listen_addr}");
        }
        #[cfg(unix)]
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(err) = handle_service_ipc_stream(stream, root, state.clone()) {
                        eprintln!("ctox service request error: {err}");
                    }
                }
                Err(err) => {
                    eprintln!(
                        "ctox service accept error on {}: {err}",
                        socket_path.display()
                    );
                    break;
                }
            }
        }
        #[cfg(not(unix))]
        for request in server.incoming_requests() {
            if let Err(err) = handle_request(request, root, state.clone()) {
                eprintln!("ctox service request error: {err}");
            }
        }
        eprintln!("ctox service accept loop ended unexpectedly; retrying bind");
        thread::sleep(Duration::from_millis(250));
    }
}

fn run_boot_state_invariant_check(root: &Path, state: &Arc<Mutex<SharedState>>) {
    run_plan_routing_repair(root, state, "boot");
    // P2 — flush any mission_state field-clobber attempts that the guard
    // suppressed during pre-boot writes (the previous run may have
    // ended without flushing if it crashed before the turn-end pass).
    lcm::drain_pending_mission_state_clobber_events_to_governance(root);
    match state_invariants::evaluate_runtime_state_invariants(root, turn_loop::CHAT_CONVERSATION_ID)
    {
        Ok(report) => {
            let violation_codes = report
                .violations
                .iter()
                .map(|violation| violation.code.clone())
                .collect::<Vec<_>>();
            if violation_codes.is_empty() {
                push_event(state, "State invariants clean at boot".to_string());
                let _ = governance::record_event(
                    root,
                    governance::GovernanceEventRequest {
                        mechanism_id: "state_invariant_guard",
                        conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                        severity: "info",
                        reason: "boot_state_invariants_clean",
                        action_taken: "recorded_state_integrity_snapshot",
                        details: serde_json::json!({
                            "violation_codes": [],
                            "open_queue_count": report.open_queue_count,
                            "open_plan_count": report.open_plan_count,
                            "continuity_focus_head_commit_id": report.continuity_focus_head_commit_id,
                        }),
                        idempotence_key: Some("boot_state_invariants_clean"),
                    },
                );
            } else {
                let mut repair_error: Option<String> = None;
                let repair_outcome = if has_repairable_state_invariants(&violation_codes) {
                    match attempt_state_invariant_repair(root, turn_loop::CHAT_CONVERSATION_ID) {
                        Ok(outcome) => Some(outcome),
                        Err(err) => {
                            repair_error = Some(err.to_string());
                            None
                        }
                    }
                } else {
                    None
                };
                if let Some((repair, repaired_report)) = &repair_outcome {
                    let repaired_codes = repaired_report
                        .violations
                        .iter()
                        .map(|violation| violation.code.clone())
                        .collect::<Vec<_>>();
                    if repaired_codes.is_empty() {
                        push_event(
                            state,
                            format!(
                                "State invariants repaired at boot: {}",
                                violation_codes.join(", ")
                            ),
                        );
                        let _ = governance::record_event(
                            root,
                            governance::GovernanceEventRequest {
                                mechanism_id: "state_invariant_guard",
                                conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                                severity: "info",
                                reason: "boot_state_invariants_repaired",
                                action_taken: state_invariant_repair_action(repair),
                                details: serde_json::json!({
                                    "violation_codes_before": violation_codes,
                                    "violation_codes_after": repaired_codes,
                                    "violations_before": report.violations,
                                    "mission_state_before": report.mission_state,
                                    "mission_state_after": repaired_report.mission_state,
                                    "continuity_focus_head_commit_id_before": report.continuity_focus_head_commit_id,
                                    "continuity_focus_head_commit_id_after": repaired_report.continuity_focus_head_commit_id,
                                    "focus_repaired": repair.focus_repaired,
                                    "reopened_for_open_runtime_work": repair.reopened_for_open_runtime_work,
                                    "previous_focus_head_commit_id": repair.previous_focus_head_commit_id,
                                    "focus_head_commit_id": repair.focus_head_commit_id,
                                }),
                                idempotence_key: Some("boot_state_invariants_repaired"),
                            },
                        );
                        return;
                    }
                }
                push_event(
                    state,
                    format!("State invariants at boot: {}", violation_codes.join(", ")),
                );
                let _ = governance::record_event(
                    root,
                    governance::GovernanceEventRequest {
                        mechanism_id: "state_invariant_guard",
                        conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                        severity: "warning",
                        reason: "boot_state_invariants_violation",
                        action_taken: "recorded_state_integrity_alert",
                        details: serde_json::json!({
                            "violation_codes": violation_codes,
                            "violations": report.violations,
                            "open_queue_count": report.open_queue_count,
                            "open_plan_count": report.open_plan_count,
                            "open_work_titles": report.open_work_titles,
                            "mission_state": report.mission_state,
                            "continuity_focus_head_commit_id": report.continuity_focus_head_commit_id,
                            "repair_attempted": repair_outcome.is_some() || repair_error.is_some(),
                            "repair_error": repair_error,
                            "post_repair_violation_codes": repair_outcome.as_ref().map(|(_, repaired_report)| {
                                repaired_report.violations.iter().map(|violation| violation.code.clone()).collect::<Vec<_>>()
                            }),
                            "post_repair_focus_repaired": repair_outcome.as_ref().map(|(repair, _)| repair.focus_repaired),
                        }),
                        idempotence_key: Some("boot_state_invariants_violation"),
                    },
                );
            }
        }
        Err(err) => {
            let error_text = clip_text(&err.to_string(), 180);
            let (reason, severity, summary) = if error_text
                .contains("missing stored narrative continuity document")
                || error_text.contains("missing stored anchors continuity document")
                || error_text.contains("missing stored focus continuity document")
            {
                (
                    "boot_state_invariants_not_ready",
                    "info",
                    "State invariants skipped at boot: continuity not initialized yet".to_string(),
                )
            } else {
                (
                    "boot_state_invariants_check_error",
                    "warning",
                    format!("State invariants skipped at boot: {error_text}"),
                )
            };
            push_event(state, summary);
            let _ = governance::record_event(
                root,
                governance::GovernanceEventRequest {
                    mechanism_id: "state_invariant_guard",
                    conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                    severity,
                    reason,
                    action_taken: "recorded_state_integrity_skip",
                    details: serde_json::json!({
                        "error": err.to_string(),
                    }),
                    idempotence_key: Some(reason),
                },
            );
        }
    }
}

fn run_boot_auto_submitted_reclassifier(root: &Path, state: &Arc<Mutex<SharedState>>) {
    match channels::reclassify_historical_auto_submitted_inbounds(root) {
        Ok(count) if count > 0 => push_event(
            state,
            format!("Boot reclassified {count} historical auto-submitted inbound(s) as terminal NO-SEND"),
        ),
        Ok(_) => {}
        Err(err) => push_event(
            state,
            format!("Boot auto-submitted reclassifier failed: {err}"),
        ),
    }
}

fn has_repairable_state_invariants(violation_codes: &[String]) -> bool {
    violation_codes.iter().any(|code| {
        matches!(
            code.as_str(),
            "mission_focus_head_mismatch"
                | "mission_state_requires_continuity_resync"
                | "focus_semantic_conflict"
                | "closed_mission_with_open_runtime_work"
                | "idle_allowed_with_open_runtime_work"
        )
    })
}

fn state_invariant_repair_action(repair: &lcm::MissionStateRepairOutcome) -> &'static str {
    if repair.focus_repaired && repair.reopened_for_open_runtime_work {
        "canonicalized_focus_and_reopened_mission_state"
    } else if repair.focus_repaired {
        "canonicalized_focus_and_resynced_mission_state"
    } else if repair.reopened_for_open_runtime_work {
        "reopened_mission_state_for_open_runtime_work"
    } else {
        "resynced_mission_state_from_continuity"
    }
}

fn primary_open_runtime_title(
    report: &state_invariants::RuntimeStateInvariantReport,
) -> Option<String> {
    report
        .open_plan_titles
        .iter()
        .rev()
        .find(|title| !title.trim().is_empty())
        .cloned()
        .or_else(|| {
            report
                .open_queue_titles
                .iter()
                .rev()
                .find(|title| !title.trim().is_empty())
                .cloned()
        })
}

fn hydrate_sparse_open_mission_state_from_runtime(
    report: &state_invariants::RuntimeStateInvariantReport,
    record: &mut lcm::MissionStateRecord,
) {
    if !record.is_open {
        return;
    }
    let Some(primary_title) = primary_open_runtime_title(report) else {
        return;
    };
    if record.mission.trim().is_empty() {
        record.mission = primary_title.clone();
    }
    if record.next_slice.trim().is_empty() {
        record.next_slice = primary_title;
    }
}

fn attempt_state_invariant_repair(
    root: &Path,
    conversation_id: i64,
) -> Result<(
    lcm::MissionStateRepairOutcome,
    state_invariants::RuntimeStateInvariantReport,
)> {
    let db_path = crate::paths::core_db(&root);
    let engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;
    let mut repair = engine.sync_mission_state_from_continuity_with_repair(conversation_id)?;
    let mut report = state_invariants::evaluate_runtime_state_invariants(root, conversation_id)?;
    let repaired_codes = report
        .violations
        .iter()
        .map(|violation| violation.code.as_str())
        .collect::<Vec<_>>();
    if repaired_codes.iter().any(|code| {
        matches!(
            *code,
            "closed_mission_with_open_runtime_work" | "idle_allowed_with_open_runtime_work"
        )
    }) && (report.open_plan_count > 0 || report.open_queue_count > 0)
    {
        let mut record = report.mission_state.clone();
        record.is_open = true;
        record.allow_idle = false;
        let mission_status = normalize_state_token(&record.mission_status);
        if matches!(
            mission_status.as_str(),
            "done" | "closed" | "complete" | "completed"
        ) {
            record.mission_status = "active".to_string();
        }
        let continuation_mode = normalize_state_token(&record.continuation_mode);
        if matches!(continuation_mode.as_str(), "closed" | "dormant") {
            record.continuation_mode = "continuous".to_string();
        }
        let closure_confidence = normalize_state_token(&record.closure_confidence);
        if matches!(
            closure_confidence.as_str(),
            "complete" | "completed" | "high"
        ) {
            record.closure_confidence = "low".to_string();
        }
        hydrate_sparse_open_mission_state_from_runtime(&report, &mut record);
        record.last_synced_at = now_iso_string();
        engine.overwrite_mission_state(&record)?;
        repair.mission_state = record;
        repair.reopened_for_open_runtime_work = true;
        report = state_invariants::evaluate_runtime_state_invariants(root, conversation_id)?;
    }
    let repaired_codes = report
        .violations
        .iter()
        .map(|violation| violation.code.as_str())
        .collect::<Vec<_>>();
    if repaired_codes == vec!["mission_state_requires_continuity_resync"]
        && report.mission_state.is_open
        && (report.open_plan_count > 0 || report.open_queue_count > 0)
    {
        let mut repaired_record = report.mission_state.clone();
        hydrate_sparse_open_mission_state_from_runtime(&report, &mut repaired_record);
        let repaired_focus = engine.rewrite_focus_continuity_from_mission_state(
            conversation_id,
            &repaired_record,
            "Rebuilt focus continuity from the current runtime state after turn-end continuity refresh was skipped.",
        )?;
        if repaired_focus {
            repair.focus_repaired = true;
            report = state_invariants::evaluate_runtime_state_invariants(root, conversation_id)?;
            repair.mission_state = report.mission_state.clone();
            repair.focus_head_commit_id = report.continuity_focus_head_commit_id.clone();
        }
    }
    Ok((repair, report))
}

fn run_turn_end_state_invariant_check(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    conversation_id: i64,
) -> Option<lcm::MissionStateRecord> {
    run_plan_routing_repair(root, state, "turn");
    // P2 — flush any mission_state field-clobber attempts that the guard
    // suppressed during the just-finished turn. Done at turn-end (and at
    // boot) so the audit trail catches them on the same DB connection
    // pass that records the rest of the post-turn governance updates.
    lcm::drain_pending_mission_state_clobber_events_to_governance(root);
    match state_invariants::evaluate_runtime_state_invariants(root, conversation_id) {
        Ok(report) => {
            let violation_codes = report
                .violations
                .iter()
                .map(|violation| violation.code.clone())
                .collect::<Vec<_>>();
            if violation_codes.is_empty() {
                return Some(report.mission_state);
            }

            let mut repair_error: Option<String> = None;
            let repair_outcome = if has_repairable_state_invariants(&violation_codes) {
                match attempt_state_invariant_repair(root, conversation_id) {
                    Ok(outcome) => Some(outcome),
                    Err(err) => {
                        repair_error = Some(err.to_string());
                        None
                    }
                }
            } else {
                None
            };

            if let Some((repair, repaired_report)) = &repair_outcome {
                let repaired_codes = repaired_report
                    .violations
                    .iter()
                    .map(|violation| violation.code.clone())
                    .collect::<Vec<_>>();
                if repaired_codes.is_empty() {
                    push_event(
                        state,
                        format!(
                            "State invariants repaired after turn: {}",
                            violation_codes.join(", ")
                        ),
                    );
                    let details = serde_json::json!({
                        "violation_codes_before": violation_codes,
                        "violation_codes_after": repaired_codes,
                        "violations_before": report.violations,
                        "mission_state_before": report.mission_state,
                        "mission_state_after": repaired_report.mission_state,
                        "continuity_focus_head_commit_id_before": report.continuity_focus_head_commit_id,
                        "continuity_focus_head_commit_id_after": repaired_report.continuity_focus_head_commit_id,
                        "focus_repaired": repair.focus_repaired,
                        "reopened_for_open_runtime_work": repair.reopened_for_open_runtime_work,
                        "previous_focus_head_commit_id": repair.previous_focus_head_commit_id,
                        "focus_head_commit_id": repair.focus_head_commit_id,
                    });
                    let _ = governance::record_event(
                        root,
                        governance::GovernanceEventRequest {
                            mechanism_id: "state_invariant_guard",
                            conversation_id: Some(conversation_id),
                            severity: "info",
                            reason: "turn_state_invariants_repaired",
                            action_taken: state_invariant_repair_action(repair),
                            details,
                            idempotence_key: None,
                        },
                    );
                    return Some(repaired_report.mission_state.clone());
                }
            }

            push_event(
                state,
                format!(
                    "State invariants after turn: {}",
                    violation_codes.join(", ")
                ),
            );
            let _ = governance::record_event(
                root,
                governance::GovernanceEventRequest {
                    mechanism_id: "state_invariant_guard",
                    conversation_id: Some(conversation_id),
                    severity: "warning",
                    reason: "turn_state_invariants_violation",
                    action_taken: "recorded_state_integrity_alert",
                    details: serde_json::json!({
                        "violation_codes": violation_codes,
                        "violations": report.violations,
                        "open_queue_count": report.open_queue_count,
                        "open_plan_count": report.open_plan_count,
                        "open_work_titles": report.open_work_titles,
                        "mission_state": report.mission_state,
                        "continuity_focus_head_commit_id": report.continuity_focus_head_commit_id,
                        "repair_attempted": repair_outcome.is_some() || repair_error.is_some(),
                        "repair_error": repair_error,
                        "post_repair_violation_codes": repair_outcome.as_ref().map(|(_, repaired_report)| {
                            repaired_report.violations.iter().map(|violation| violation.code.clone()).collect::<Vec<_>>()
                        }),
                        "post_repair_focus_repaired": repair_outcome.as_ref().map(|(repair, _)| repair.focus_repaired),
                    }),
                    idempotence_key: None,
                },
            );

            Some(
                repair_outcome
                    .map(|(_, repaired_report)| repaired_report.mission_state)
                    .unwrap_or(report.mission_state),
            )
        }
        Err(err) => {
            let error_text = clip_text(&err.to_string(), 180);
            let (reason, severity, summary) = if error_text
                .contains("missing stored narrative continuity document")
                || error_text.contains("missing stored anchors continuity document")
                || error_text.contains("missing stored focus continuity document")
            {
                (
                    "turn_state_invariants_not_ready",
                    "info",
                    "State invariants skipped after turn: continuity not initialized yet"
                        .to_string(),
                )
            } else {
                (
                    "turn_state_invariants_check_error",
                    "warning",
                    format!("State invariants skipped after turn: {error_text}"),
                )
            };
            push_event(state, summary);
            let _ = governance::record_event(
                root,
                governance::GovernanceEventRequest {
                    mechanism_id: "state_invariant_guard",
                    conversation_id: Some(conversation_id),
                    severity,
                    reason,
                    action_taken: "recorded_state_integrity_skip",
                    details: serde_json::json!({
                        "error": err.to_string(),
                    }),
                    idempotence_key: None,
                },
            );
            None
        }
    }
}

fn release_stale_service_communication_leases_on_boot(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
) {
    match release_stale_service_communication_leases(root) {
        Ok(0) => {}
        Ok(count) => push_event(
            state,
            format!("Released {count} stale service communication lease(s) at boot"),
        ),
        Err(err) => push_event(
            state,
            format!("Boot lease repair failed for communication routes: {err}"),
        ),
    }
}

fn release_stale_service_communication_leases(root: &Path) -> Result<usize> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let now = now_iso_string();
    let updated = conn.execute(
        r#"
        UPDATE communication_routing_state
        SET route_status='pending',
            lease_owner=NULL,
            leased_at=NULL,
            last_error='released stale service lease during service boot',
            updated_at=?1
        WHERE route_status='leased'
          AND lease_owner=?2
          AND acked_at IS NULL
        "#,
        params![now, CHANNEL_ROUTER_LEASE_OWNER],
    )?;
    Ok(updated)
}

fn is_non_work_tui_probe(prompt: &str) -> bool {
    let normalized = prompt
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "hello queue" | "hello" | "ping" | "healthcheck" | "health check"
    )
}

fn start_work_hours_dispatcher(root: PathBuf, state: Arc<Mutex<SharedState>>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(60));
        if !crate::service::working_hours::accepts_work(&root) {
            continue;
        }
        let next_prompt = {
            let mut shared = lock_shared_state(&state);
            if shared.busy || runtime_blocker_backoff_remaining_secs(&shared).is_some() {
                None
            } else {
                maybe_start_next_queued_prompt_locked(&root, &mut shared)
            }
        };
        if let Some(queued) = next_prompt {
            push_event(
                &state,
                "Working-hours window open; resuming queued work".to_string(),
            );
            start_prompt_worker(root.clone(), state.clone(), queued);
        }
    });
}

fn run_plan_routing_repair(root: &Path, state: &Arc<Mutex<SharedState>>, phase: &str) {
    match plan::repair_stale_step_routing_state(root) {
        Ok(repaired) if repaired > 0 => {
            push_event(
                state,
                format!(
                    "Repaired {repaired} stale plan routing {} at {phase}",
                    if repaired == 1 { "entry" } else { "entries" }
                ),
            );
            let _ = governance::record_event(
                root,
                governance::GovernanceEventRequest {
                    mechanism_id: "plan_routing_repair",
                    conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                    severity: "info",
                    reason: "stale_plan_routing_repaired",
                    action_taken: "released_or_closed_stale_plan_queue_routes",
                    details: serde_json::json!({
                        "phase": phase,
                        "repaired_count": repaired,
                    }),
                    idempotence_key: None,
                },
            );
        }
        Ok(_) => {}
        Err(err) => {
            push_event(
                state,
                format!(
                    "Plan routing repair skipped at {phase}: {}",
                    clip_text(&err.to_string(), 180)
                ),
            );
        }
    }
}

fn normalize_state_token(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn start_background(root: &Path) -> Result<String> {
    if let Some(reason) = crate::service::working_hours::hold_reason(root) {
        return Ok(format!("CTOX service not started: {reason}"));
    }
    if let Some(systemd) = systemd_unit_status(root)? {
        if systemd.active {
            return Ok(format!(
                "CTOX service already running via systemd user unit on {}",
                service_listen_addr(root)
            ));
        }
        cleanup_stale_service_runtime(root)?;
        systemctl_user(["daemon-reload"])?;
        systemctl_user(["enable", SYSTEMD_USER_UNIT_NAME])?;
        systemctl_user(["start", SYSTEMD_USER_UNIT_NAME])?;
        // After an upgrade the freshly-deployed binary needs noticeably
        // longer to settle (cargo-built artefact, on-disk caches cold,
        // SQLite migrations, model registry boot).  The previous 6 s
        // timeout silently returned `Ok` when the service had not actually
        // come up, leaving the caller (the upgrade pipeline) believing
        // the daemon was running while production stayed down.  60 s with
        // a hard error on miss closes that silent-failure window.
        let attempts: usize = 200;
        let interval = Duration::from_millis(300);
        for _ in 0..attempts {
            thread::sleep(interval);
            let status = service_status_snapshot(root)?;
            if status.running {
                return Ok(format!(
                    "CTOX service enabled and started via systemd on {}",
                    status.listen_addr
                ));
            }
        }
        anyhow::bail!(
            "CTOX systemd service did not come up within {:?} of `systemctl --user start {}`. Inspect `journalctl --user -u {}` for the boot failure.",
            interval * (attempts as u32),
            SYSTEMD_USER_UNIT_NAME,
            SYSTEMD_USER_UNIT_NAME,
        );
    }
    let status = service_status_snapshot(root)?;
    if status.running {
        return Ok(format!(
            "CTOX service already running on {}",
            status.listen_addr
        ));
    }
    cleanup_stale_service_runtime(root)?;
    if let Some(pid_path_parent) = service_pid_path(root).parent() {
        std::fs::create_dir_all(pid_path_parent).with_context(|| {
            format!("failed to create runtime dir {}", pid_path_parent.display())
        })?;
    }
    let _ = std::fs::remove_file(service_pid_path(root));
    let log_path = service_log_path(root);
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open service log {}", log_path.display()))?;
    let log_file_err = log_file
        .try_clone()
        .with_context(|| format!("failed to clone service log {}", log_path.display()))?;
    let exe = preferred_ctox_executable(root)?;
    let mut command = Command::new(&exe);
    command
        .arg("service")
        .arg("--foreground")
        .current_dir(root)
        .env("CTOX_ROOT", root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err));
    configure_background_service_process(&mut command);
    let child = command
        .spawn()
        .context("failed to spawn detached CTOX service")?;
    let _ = write_pid_file(root, child.id());
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        let status = service_status_snapshot(root)?;
        if status.running {
            return Ok(format!(
                "CTOX service started on {}. Log: {}",
                status.listen_addr,
                log_path.display()
            ));
        }
    }
    Ok(format!(
        "CTOX service spawn requested. Check {} for startup logs.",
        log_path.display()
    ))
}

#[cfg(unix)]
fn configure_background_service_process(command: &mut Command) {
    unsafe {
        command.pre_exec(|| {
            if setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            signal(SIGPIPE, SIG_IGN);
            let mut current = rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if getrlimit(RLIMIT_NOFILE, &mut current) == 0 {
                let target = 65_535 as libc::rlim_t;
                let raised = rlimit {
                    rlim_cur: std::cmp::min(target, current.rlim_max),
                    rlim_max: current.rlim_max,
                };
                let _ = setrlimit(RLIMIT_NOFILE, &raised);
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure_background_service_process(_command: &mut Command) {}

pub fn stop_background(root: &Path) -> Result<String> {
    let preflight_backend_shutdown_error = supervisor::shutdown_persistent_backends(root)
        .err()
        .map(|err| err.to_string());
    let had_service_processes = !matching_service_processes(root, None)?.is_empty();
    let had_live_service_pid = read_pid_file(root).map(process_is_running).unwrap_or(false);
    let had_backends = !supervisor::persistent_backends_idle(root)?;
    if let Some(systemd) = systemd_unit_status(root)? {
        let had_systemd_service = systemd.active || systemd.enabled || systemd.pid.is_some();
        let mut systemd_failures = Vec::new();
        if systemd.active || systemd.enabled {
            if let Err(err) = systemctl_user(["stop", SYSTEMD_USER_UNIT_NAME]) {
                systemd_failures.push(format!("systemd stop: {err}"));
            }
            if let Err(err) = systemctl_user(["disable", SYSTEMD_USER_UNIT_NAME]) {
                systemd_failures.push(format!("systemd disable: {err}"));
            }
        }
        let _ = std::fs::remove_file(service_pid_path(root));
        if let Some(err) = preflight_backend_shutdown_error.as_ref() {
            eprintln!("ctox preflight backend shutdown reported residue: {err}");
        }
        let cleaned = cleanup_orphan_service_processes(root, None)?;
        if wait_for_service_shutdown(root, Duration::from_secs(SERVICE_SHUTDOWN_TIMEOUT_SECS))? {
            if !supervisor::persistent_backends_idle(root)? {
                anyhow::bail!(
                    "CTOX service stop did not complete cleanly: {}",
                    supervisor::persistent_backend_alerts(root)?.join("; ")
                );
            }
            if had_systemd_service || had_service_processes || had_live_service_pid || had_backends
            {
                return Ok("CTOX service stopped and disabled.".to_string());
            }
            return Ok("CTOX service is already stopped and disabled.".to_string());
        }
        let mut residue = service_shutdown_residue(root)?;
        if let Some(err) = preflight_backend_shutdown_error {
            systemd_failures.push(format!("backend preflight: {err}"));
        }
        if cleaned > 0 {
            systemd_failures.push(format!(
                "service fallback signaled {cleaned} foreground process(es)"
            ));
        }
        systemd_failures.append(&mut residue);
        anyhow::bail!(
            "CTOX service stop did not complete cleanly: {}",
            systemd_failures.join("; ")
        );
    }
    let status = service_status_snapshot(root)?;
    if status.running {
        #[cfg(unix)]
        {
            let _ = send_service_ipc_request(root, ServiceIpcRequest::Stop);
        }
        #[cfg(not(unix))]
        {
            let url = format!("{}/ctox/service/stop", service_base_url(root));
            let _ = ureq::post(&url)
                .set("content-type", "application/json")
                .send_string("{}");
        }
        if wait_for_service_shutdown(root, Duration::from_secs(SERVICE_SHUTDOWN_TIMEOUT_SECS))? {
            return Ok("CTOX service stopped.".to_string());
        }
    }
    if let Some(pid) = read_pid_file(root) {
        let status = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .with_context(|| format!("failed to signal CTOX service pid {pid}"))?;
        if !status.success() {
            anyhow::bail!("failed to stop CTOX service pid {pid}");
        }
        let _ = std::fs::remove_file(service_pid_path(root));
    }
    let cleaned = cleanup_orphan_service_processes(root, None)?;
    if let Some(err) = preflight_backend_shutdown_error.as_ref() {
        eprintln!("ctox preflight backend shutdown reported residue: {err}");
    }
    if wait_for_service_shutdown(root, Duration::from_secs(SERVICE_SHUTDOWN_TIMEOUT_SECS))? {
        if !supervisor::persistent_backends_idle(root)? {
            anyhow::bail!(
                "CTOX service stop did not complete cleanly: {}",
                supervisor::persistent_backend_alerts(root)?.join("; ")
            );
        }
        if had_service_processes || had_live_service_pid || cleaned > 0 || had_backends {
            if cleaned > 0 {
                return Ok(format!(
                    "CTOX service pid file was missing, but {cleaned} orphaned service process(es) were signaled for shutdown."
                ));
            }
            return Ok("CTOX service stopped.".to_string());
        }
        return Ok("CTOX service is not running.".to_string());
    }
    anyhow::bail!(
        "CTOX service stop did not complete cleanly: {}",
        service_shutdown_residue(root)?.join("; ")
    )
}

pub fn submit_chat_prompt(root: &Path, prompt: &str) -> Result<()> {
    submit_chat_prompt_with_thread_key(root, prompt, None)
}

/// Operator-supplied outbound-email intent attached to a chat submission.
///
/// When present, the agent's reply will be routed through the reviewed
/// founder-outbound pipeline if (and only if) at least one recipient is
/// classified as owner/founder/admin per the deterministic
/// `protected_recipient_policies` check. There is no text-scraping fallback.
#[derive(Debug, Clone)]
pub struct OutboundEmailIntent {
    pub account_key: String,
    pub thread_key: String,
    pub subject: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub attachments: Vec<String>,
}

impl From<OutboundEmailIntent> for channels::FounderOutboundAction {
    fn from(value: OutboundEmailIntent) -> Self {
        channels::FounderOutboundAction {
            account_key: value.account_key,
            thread_key: value.thread_key,
            subject: value.subject,
            to: value.to,
            cc: value.cc,
            attachments: value.attachments,
        }
    }
}

pub fn prepare_chat_prompt(root: &Path, prompt: &str) -> Result<PreparedChatPrompt> {
    let sanitized = secrets::auto_intake_prompt_secrets(root, prompt)?;
    Ok(PreparedChatPrompt {
        prompt: sanitized.sanitized_prompt,
        auto_ingested_secrets: sanitized.auto_ingested_secrets,
        suggested_skill: (sanitized.auto_ingested_secrets > 0)
            .then(|| "secret-hygiene".to_string()),
    })
}

pub fn submit_chat_prompt_with_thread_key(
    root: &Path,
    prompt: &str,
    thread_key: Option<&str>,
) -> Result<()> {
    submit_chat_prompt_with_intent(root, prompt, thread_key, None)
}

pub fn submit_chat_prompt_with_intent(
    root: &Path,
    prompt: &str,
    thread_key: Option<&str>,
    outbound_email: Option<OutboundEmailIntent>,
) -> Result<()> {
    let prepared = prepare_chat_prompt(root, prompt)?;
    let outbound_email = outbound_email.map(channels::FounderOutboundAction::from);
    // TUI-initiated proactive outbound has no leased inbound message key,
    // so without an explicit anchor the post-turn dispatcher cannot match
    // the review approval to the draft. Mint a synthetic anchor here, at
    // the structural boundary where we know the call originated from the
    // TUI submit path. The format `tui-outbound:<uuid>` is reserved for
    // this purpose and never derived from prompt content.
    let outbound_anchor = outbound_email
        .as_ref()
        .map(|_| format!("tui-outbound:{}", uuid::Uuid::new_v4()));
    #[cfg(unix)]
    {
        match send_service_ipc_request(
            root,
            ServiceIpcRequest::ChatSubmit {
                prompt: prepared.prompt,
                thread_key: thread_key.map(str::to_owned),
                outbound_email,
                outbound_anchor,
            },
        )? {
            ServiceIpcResponse::Accepted(_) => return Ok(()),
            ServiceIpcResponse::Error { message } => anyhow::bail!(message),
            other => anyhow::bail!("unexpected CTOX service reply: {other:?}"),
        }
    }
    #[cfg(not(unix))]
    {
        let url = format!("{}/ctox/service/chat", service_base_url(root));
        let payload = serde_json::to_string(&ChatSubmitRequest {
            prompt: prepared.prompt,
            thread_key: thread_key.map(str::to_owned),
            outbound_email,
            outbound_anchor,
        })?;
        let response = ureq::post(&url)
            .set("content-type", "application/json")
            .send_string(&payload)
            .with_context(|| format!("failed to reach CTOX service at {url}"))?;
        if response.status() >= 300 {
            anyhow::bail!("CTOX service rejected the chat request");
        }
        Ok(())
    }
}

pub fn service_status_snapshot(root: &Path) -> Result<ServiceStatus> {
    let _ = runtime_control::reconcile_runtime_switch_transaction(root);
    let systemd = systemd_unit_status(root)?;
    if let Some(mut status) = live_service_status_snapshot(root)? {
        if let Some(systemd) = systemd {
            status.autostart_enabled = systemd.enabled;
            if systemd.active {
                status.manager = "systemd-user".to_string();
                status.pid = systemd.pid.or(status.pid);
            } else {
                status.manager = "process".to_string();
            }
        } else {
            status.autostart_enabled = false;
            status.manager = "process".to_string();
        }
        status.running = true;
        status.monitor_alerts = runtime_lifecycle_alerts(root, status.pid, true)?;
        return Ok(status);
    }
    if let Some(systemd) = systemd {
        let mut status = ServiceStatus::stopped(root);
        status.running = systemd.active;
        status.pid = systemd.pid.or(status.pid);
        status.autostart_enabled = systemd.enabled;
        status.manager = "systemd-user".to_string();
        status.monitor_alerts = runtime_lifecycle_alerts(root, status.pid, status.running)?;
        return Ok(status);
    }
    #[cfg(unix)]
    {
        return Ok(ServiceStatus::stopped(root));
    }
    #[cfg(not(unix))]
    {
        Ok(ServiceStatus::stopped(root))
    }
}

fn live_service_status_snapshot(root: &Path) -> Result<Option<ServiceStatus>> {
    #[cfg(unix)]
    {
        match send_service_ipc_request(root, ServiceIpcRequest::Status) {
            Ok(ServiceIpcResponse::Status(status)) => Ok(Some(status)),
            Ok(_) | Err(_) => Ok(None),
        }
    }
    #[cfg(not(unix))]
    {
        let status_agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_millis(100))
            .timeout_read(Duration::from_millis(150))
            .timeout_write(Duration::from_millis(150))
            .build();
        let url = format!("{}/ctox/service/status", service_base_url(root));
        let response = match status_agent.get(&url).call() {
            Ok(response) => response,
            Err(_) => return Ok(None),
        };
        let body = response
            .into_string()
            .context("failed to read CTOX service status response")?;
        Ok(Some(parse_service_status(&body, root)?))
    }
}

#[cfg(not(unix))]
pub fn service_base_url(root: &Path) -> String {
    format!("http://{}", service_listen_addr(root))
}

#[cfg(unix)]
fn handle_service_ipc_stream(
    stream: UnixStream,
    root: &Path,
    state: Arc<Mutex<SharedState>>,
) -> Result<()> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .context("failed to clone service socket")?,
    );
    let mut request_line = String::new();
    let read = reader
        .read_line(&mut request_line)
        .context("failed to read service socket request")?;
    if read == 0 {
        return Ok(());
    }
    let request: ServiceIpcRequest = serde_json::from_str(request_line.trim())
        .context("failed to parse service socket request")?;
    let response = handle_service_ipc_request(request, root, state)?;
    let mut writer = BufWriter::new(stream);
    let payload =
        serde_json::to_vec(&response).context("failed to encode service socket response")?;
    writer
        .write_all(&payload)
        .context("failed to write service socket response")?;
    writer
        .write_all(b"\n")
        .context("failed to terminate service socket response")?;
    writer
        .flush()
        .context("failed to flush service socket response")
}

#[cfg(unix)]
fn handle_service_ipc_request(
    request: ServiceIpcRequest,
    root: &Path,
    state: Arc<Mutex<SharedState>>,
) -> Result<ServiceIpcResponse> {
    match request {
        ServiceIpcRequest::Status => Ok(ServiceIpcResponse::Status(status_from_shared_state(
            root, &state,
        )?)),
        ServiceIpcRequest::ChatSubmit {
            prompt,
            thread_key,
            outbound_email,
            outbound_anchor,
        } => {
            let prepared = prepare_chat_prompt(root, &prompt)?;
            let prompt = prepared.prompt;
            if is_non_work_tui_probe(&prompt) {
                push_event(&state, "Ignored non-work TUI probe".to_string());
                return Ok(ServiceIpcResponse::Accepted(AcceptedResponse {
                    accepted: true,
                    status: "ignored".to_string(),
                }));
            }
            let suggested_skill = prepared.suggested_skill.clone();
            let workspace_root = channels::legacy_workspace_root_from_prompt(&prompt);
            let queued = {
                let mut shared = lock_shared_state(&state);
                if let Some(reason) = crate::service::working_hours::hold_reason(root) {
                    insert_pending_prompt_ordered(
                        &mut shared.pending_prompts,
                        QueuedPrompt {
                            preview: preview_text(&prompt),
                            source_label: "tui".to_string(),
                            goal: prompt.clone(),
                            prompt: prompt.clone(),
                            suggested_skill: suggested_skill.clone(),
                            leased_message_keys: Vec::new(),
                            leased_ticket_event_keys: Vec::new(),
                            thread_key: thread_key.clone(),
                            workspace_root: workspace_root.clone(),
                            ticket_self_work_id: None,
                            outbound_email: outbound_email.clone(),
                            outbound_anchor: outbound_anchor.clone(),
                        },
                    );
                    let pending = shared.pending_prompts.len();
                    push_event_locked(
                        &mut shared,
                        format!("Queued prompt outside working hours (queue #{pending}): {reason}"),
                    );
                    true
                } else if shared.busy || runtime_blocker_backoff_remaining_secs(&shared).is_some() {
                    insert_pending_prompt_ordered(
                        &mut shared.pending_prompts,
                        QueuedPrompt {
                            preview: preview_text(&prompt),
                            source_label: "tui".to_string(),
                            goal: prompt.clone(),
                            prompt: prompt.clone(),
                            suggested_skill: suggested_skill.clone(),
                            leased_message_keys: Vec::new(),
                            leased_ticket_event_keys: Vec::new(),
                            thread_key: thread_key.clone(),
                            workspace_root: workspace_root.clone(),
                            ticket_self_work_id: None,
                            outbound_email: outbound_email.clone(),
                            outbound_anchor: outbound_anchor.clone(),
                        },
                    );
                    ensure_queue_guard_locked(root, &mut shared);
                    let pending = shared.pending_prompts.len();
                    let reason = runtime_blocker_backoff_remaining_secs(&shared)
                        .map(|secs| format!("runtime blocker cooldown {secs}s"))
                        .unwrap_or_else(|| "service busy".to_string());
                    push_event_locked(
                        &mut shared,
                        decorate_service_event_with_skill(
                            &format!("Queued follow-up prompt #{pending} ({reason})"),
                            suggested_skill.as_deref(),
                        ),
                    );
                    true
                } else {
                    shared.busy = true;
                    shared.current_goal_preview = Some(preview_text(&prompt));
                    shared.active_source_label = Some("tui".to_string());
                    shared.last_error = None;
                    shared.last_reply_chars = None;
                    push_event_locked(
                        &mut shared,
                        decorate_service_event_with_skill(
                            "Started prompt",
                            suggested_skill.as_deref(),
                        ),
                    );
                    if prepared.auto_ingested_secrets > 0 {
                        push_event_locked(
                            &mut shared,
                            format!(
                                "Auto-ingested {} prompt secret(s) into the secret store",
                                prepared.auto_ingested_secrets
                            ),
                        );
                    }
                    false
                }
            };
            if !queued {
                start_prompt_worker(
                    root.to_path_buf(),
                    state.clone(),
                    QueuedPrompt {
                        preview: preview_text(&prompt),
                        source_label: "tui".to_string(),
                        goal: prompt.clone(),
                        prompt,
                        suggested_skill,
                        leased_message_keys: Vec::new(),
                        leased_ticket_event_keys: Vec::new(),
                        thread_key,
                        workspace_root,
                        ticket_self_work_id: None,
                        outbound_email,
                        outbound_anchor,
                    },
                );
            }
            Ok(ServiceIpcResponse::Accepted(AcceptedResponse {
                accepted: true,
                status: if queued { "queued" } else { "started" }.to_string(),
            }))
        }
        ServiceIpcRequest::Stop => {
            let root = root.to_path_buf();
            thread::spawn(move || {
                if let Err(err) = supervisor::shutdown_persistent_backends(&root) {
                    eprintln!("ctox backend shutdown error during service stop: {err}");
                }
                let _ = std::fs::remove_file(service_pid_path(&root));
                let _ = std::fs::remove_file(service_socket_path(&root));
                thread::sleep(Duration::from_millis(50));
                std::process::exit(0);
            });
            Ok(ServiceIpcResponse::Accepted(AcceptedResponse {
                accepted: true,
                status: "stopping".to_string(),
            }))
        }
        ServiceIpcRequest::ScrapeApi { path } => {
            let (status, payload) = resolve_scrape_api_payload(root, &path)?;
            Ok(ServiceIpcResponse::Json { status, payload })
        }
    }
}

#[cfg(not(unix))]
fn handle_request(
    mut request: tiny_http::Request,
    root: &Path,
    state: Arc<Mutex<SharedState>>,
) -> Result<()> {
    let method = request.method().clone();
    let url = request.url().to_string();
    eprintln!("ctox service request {} {}", method.as_str(), url);
    match (method, url.as_str()) {
        (Method::Get, "/ctox/service/status") => {
            let snapshot = status_from_shared_state(root, &state)?;
            respond_json(request, StatusCode(200), &snapshot)?;
        }
        (Method::Post, "/ctox/service/chat") => {
            let mut body = String::new();
            request
                .as_reader()
                .read_to_string(&mut body)
                .context("failed to read chat request body")?;
            let payload: ChatSubmitRequest =
                serde_json::from_str(&body).context("failed to parse chat request json")?;
            let prepared = prepare_chat_prompt(root, &payload.prompt)?;
            let prompt = prepared.prompt;
            if is_non_work_tui_probe(&prompt) {
                push_event(&state, "Ignored non-work TUI probe".to_string());
                respond_json(
                    request,
                    StatusCode(202),
                    &AcceptedResponse {
                        accepted: true,
                        status: "ignored".to_string(),
                    },
                )?;
                return Ok(());
            }
            let suggested_skill = prepared.suggested_skill.clone();
            let workspace_root = channels::legacy_workspace_root_from_prompt(&prompt);
            let queued = {
                let mut shared = lock_shared_state(&state);
                if let Some(reason) = crate::service::working_hours::hold_reason(root) {
                    insert_pending_prompt_ordered(
                        &mut shared.pending_prompts,
                        QueuedPrompt {
                            preview: preview_text(&prompt),
                            source_label: "tui".to_string(),
                            goal: prompt.clone(),
                            prompt: prompt.clone(),
                            suggested_skill: suggested_skill.clone(),
                            leased_message_keys: Vec::new(),
                            leased_ticket_event_keys: Vec::new(),
                            thread_key: payload.thread_key.clone(),
                            workspace_root: workspace_root.clone(),
                            ticket_self_work_id: None,
                            outbound_email: payload.outbound_email.clone(),
                            outbound_anchor: payload.outbound_anchor.clone(),
                        },
                    );
                    let pending = shared.pending_prompts.len();
                    push_event_locked(
                        &mut shared,
                        format!("Queued prompt outside working hours (queue #{pending}): {reason}"),
                    );
                    true
                } else if shared.busy || runtime_blocker_backoff_remaining_secs(&shared).is_some() {
                    insert_pending_prompt_ordered(
                        &mut shared.pending_prompts,
                        QueuedPrompt {
                            preview: preview_text(&prompt),
                            source_label: "tui".to_string(),
                            goal: prompt.clone(),
                            prompt: prompt.clone(),
                            suggested_skill: suggested_skill.clone(),
                            leased_message_keys: Vec::new(),
                            leased_ticket_event_keys: Vec::new(),
                            thread_key: payload.thread_key.clone(),
                            workspace_root: workspace_root.clone(),
                            ticket_self_work_id: None,
                            outbound_email: payload.outbound_email.clone(),
                            outbound_anchor: payload.outbound_anchor.clone(),
                        },
                    );
                    ensure_queue_guard_locked(root, &mut shared);
                    let pending = shared.pending_prompts.len();
                    let reason = runtime_blocker_backoff_remaining_secs(&shared)
                        .map(|secs| format!("runtime blocker cooldown {secs}s"))
                        .unwrap_or_else(|| "service busy".to_string());
                    push_event_locked(
                        &mut shared,
                        decorate_service_event_with_skill(
                            &format!("Queued follow-up prompt #{pending} ({reason})"),
                            suggested_skill.as_deref(),
                        ),
                    );
                    true
                } else {
                    shared.busy = true;
                    shared.current_goal_preview = Some(preview_text(&prompt));
                    shared.active_source_label = Some("tui".to_string());
                    shared.last_error = None;
                    shared.last_reply_chars = None;
                    push_event_locked(
                        &mut shared,
                        decorate_service_event_with_skill(
                            "Started prompt",
                            suggested_skill.as_deref(),
                        ),
                    );
                    if prepared.auto_ingested_secrets > 0 {
                        push_event_locked(
                            &mut shared,
                            format!(
                                "Auto-ingested {} prompt secret(s) into the secret store",
                                prepared.auto_ingested_secrets
                            ),
                        );
                    }
                    false
                }
            };
            if !queued {
                start_prompt_worker(
                    root.to_path_buf(),
                    state.clone(),
                    QueuedPrompt {
                        preview: preview_text(&prompt),
                        source_label: "tui".to_string(),
                        goal: prompt.clone(),
                        prompt,
                        suggested_skill,
                        leased_message_keys: Vec::new(),
                        leased_ticket_event_keys: Vec::new(),
                        thread_key: payload.thread_key,
                        workspace_root,
                        ticket_self_work_id: None,
                        outbound_email: payload.outbound_email,
                        outbound_anchor: payload.outbound_anchor,
                    },
                );
            }
            respond_json(
                request,
                StatusCode(202),
                &AcceptedResponse {
                    accepted: true,
                    status: if queued { "queued" } else { "started" }.to_string(),
                },
            )?;
        }
        (Method::Post, "/ctox/service/stop") => {
            let response = serde_json::json!({"stopping": true});
            respond_json(request, StatusCode(200), &response)?;
            let root = root.to_path_buf();
            thread::spawn(move || {
                if let Err(err) = supervisor::shutdown_persistent_backends(&root) {
                    eprintln!("ctox backend shutdown error during service stop: {err}");
                }
                let _ = std::fs::remove_file(service_pid_path(&root));
                thread::sleep(Duration::from_millis(50));
                std::process::exit(0);
            });
        }
        (Method::Get, _) if url.starts_with("/ctox/scrape/targets/") => {
            handle_scrape_api_request(request, root, &url)?;
        }
        _ => {
            respond_json(
                request,
                StatusCode(404),
                &serde_json::json!({"error": "not found"}),
            )?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn handle_scrape_api_request(
    request: tiny_http::Request,
    root: &Path,
    raw_url: &str,
) -> Result<()> {
    let (status, payload) = resolve_scrape_api_payload(root, raw_url)?;
    respond_json(request, StatusCode(status), &payload)?;
    Ok(())
}

fn resolve_scrape_api_payload(root: &Path, raw_url: &str) -> Result<(u16, serde_json::Value)> {
    let parsed = url::Url::parse(&format!("http://ctox.local{raw_url}"))
        .context("failed to parse scrape api url")?;
    let segments = parsed
        .path_segments()
        .map(|items| items.collect::<Vec<_>>())
        .unwrap_or_default();
    if segments.len() < 4
        || segments[0] != "ctox"
        || segments[1] != "scrape"
        || segments[2] != "targets"
    {
        return Ok((404, serde_json::json!({"error": "not found"})));
    }
    let target_key = segments[3];
    let action = segments.get(4).copied().unwrap_or("api");
    let query_pairs = parsed.query_pairs().into_owned().collect::<Vec<_>>();
    match action {
        "api" => match scrape::service_show_api(root, target_key)? {
            Some(payload) => Ok((200, payload)),
            None => Ok((404, serde_json::json!({"error": "target not found"}))),
        },
        "latest" => match scrape::show_latest(root, target_key, 20)? {
            Some(payload) => Ok((200, payload)),
            None => Ok((404, serde_json::json!({"error": "target not found"}))),
        },
        "records" => {
            let limit = query_pairs
                .iter()
                .find(|(key, _)| key == "limit")
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(50);
            let filters = query_pairs
                .iter()
                .filter(|(key, _)| key != "limit" && key != "q")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Vec<_>>();
            match scrape::service_query_records(root, target_key, &filters, limit)? {
                Some(payload) => Ok((200, payload)),
                None => Ok((404, serde_json::json!({"error": "target not found"}))),
            }
        }
        "semantic" => {
            let limit = query_pairs
                .iter()
                .find(|(key, _)| key == "limit")
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(12);
            let query = query_pairs
                .iter()
                .find(|(key, _)| key == "q")
                .map(|(_, value)| value.clone());
            let Some(query) = query else {
                return Ok((
                    400,
                    serde_json::json!({"error": "missing q query parameter"}),
                ));
            };
            match scrape::service_semantic_search(root, target_key, &query, limit)? {
                Some(payload) => Ok((200, payload)),
                None => Ok((404, serde_json::json!({"error": "target not found"}))),
            }
        }
        _ => Ok((
            404,
            serde_json::json!({"error": "unknown scrape api route"}),
        )),
    }
}

fn status_from_shared_state(root: &Path, state: &Arc<Mutex<SharedState>>) -> Result<ServiceStatus> {
    let shared = lock_shared_state(state);
    let busy = shared.busy;
    let pid = Some(std::process::id());
    let current_goal_preview = shared.current_goal_preview.clone();
    let active_source_label = shared.active_source_label.clone();
    let recent_events = shared.recent_events.iter().cloned().collect::<Vec<_>>();
    let last_error = shared.last_error.clone();
    let last_completed_at = shared.last_completed_at.clone();
    let last_reply_chars = shared.last_reply_chars;
    let mut pending_previews = shared
        .pending_prompts
        .iter()
        .take(6)
        .map(|item| format!("{}  {}", item.source_label, item.preview))
        .collect::<Vec<_>>();
    let in_memory_pending_count = shared.pending_prompts.len();
    drop(shared);

    let durable_tasks = channels::list_queue_tasks(
        root,
        &[
            "pending".to_string(),
            "leased".to_string(),
            "blocked".to_string(),
        ],
        6,
    )
    .unwrap_or_default();
    let ticket_cases = tickets::list_cases(root, None, 6).unwrap_or_default();
    for task in &durable_tasks {
        if pending_previews.len() >= 6 {
            break;
        }
        let preview = if task.route_status == "blocked" {
            format!("queue blocked  {}", clip_text(task.title.trim(), 112))
        } else {
            format!("queue  {}", clip_text(task.title.trim(), 120))
        };
        if !pending_previews.iter().any(|existing| existing == &preview) {
            pending_previews.push(preview);
        }
    }
    for case in ticket_cases
        .into_iter()
        .filter(|case| !matches!(case.state.as_str(), "closed"))
    {
        if pending_previews.len() >= 6 {
            break;
        }
        let preview = format!(
            "ticket  {} {}",
            case.label,
            clip_text(case.ticket_key.trim(), 96)
        );
        if !pending_previews.iter().any(|existing| existing == &preview) {
            pending_previews.push(preview);
        }
    }

    let last_agent_outcome = {
        let db_path = crate::paths::core_db(&root);
        lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())
            .ok()
            .and_then(|engine| {
                engine
                    .last_agent_outcome(turn_loop::CHAT_CONVERSATION_ID)
                    .ok()
                    .flatten()
            })
            .map(|outcome| outcome.as_str().to_string())
    };
    Ok(ServiceStatus {
        running: true,
        busy,
        pid,
        listen_addr: service_listen_addr(root),
        autostart_enabled: systemd_unit_status(root)
            .ok()
            .flatten()
            .map(|status| status.enabled)
            .unwrap_or(false),
        manager: systemd_unit_status(root)
            .ok()
            .flatten()
            .map(|_| "systemd-user".to_string())
            .unwrap_or_else(|| "process".to_string()),
        pending_count: in_memory_pending_count.max(durable_tasks.len().max(pending_previews.len())),
        pending_previews,
        current_goal_preview,
        active_source_label,
        recent_events,
        last_error,
        last_completed_at,
        last_reply_chars,
        monitor_last_check_at: None,
        monitor_alerts: runtime_lifecycle_alerts(root, pid, true)?,
        monitor_last_error: None,
        last_agent_outcome,
        work_hours: crate::service::working_hours::snapshot(root),
    })
}

fn runtime_lifecycle_alerts(
    root: &Path,
    current_pid: Option<u32>,
    service_running: bool,
) -> Result<Vec<String>> {
    let mut alerts = Vec::new();
    if let Some(pid) = read_pid_file(root) {
        if !process_is_running(pid) {
            alerts.push(format!(
                "stale service pid file {} -> {pid}",
                service_pid_path(root).display()
            ));
        } else if current_pid.is_some() && Some(pid) != current_pid {
            alerts.push(format!(
                "service pid file points at {pid}, current service pid is {:?}",
                current_pid
            ));
        }
    }
    let duplicate_service_pids = matching_service_processes(root, current_pid)?;
    if !duplicate_service_pids.is_empty() {
        alerts.push(format!(
            "duplicate service foreground processes {duplicate_service_pids:?}"
        ));
    }
    let backend_alerts = supervisor::persistent_backend_alerts(root)?;
    if !service_running && !backend_alerts.is_empty() {
        alerts.extend(
            backend_alerts
                .into_iter()
                .map(|alert| format!("backend residue {alert}")),
        );
    }
    Ok(alerts)
}

#[cfg(not(unix))]
fn respond_json<T: Serialize>(
    request: tiny_http::Request,
    status: StatusCode,
    payload: &T,
) -> Result<()> {
    let body = serde_json::to_string(payload)?;
    let response = Response::from_string(body)
        .with_status_code(status)
        .with_header(
            Header::from_bytes(b"content-type", b"application/json")
                .map_err(|_| anyhow::anyhow!("failed to build content-type header"))?,
        );
    request
        .respond(response)
        .context("failed to send service response")
}

#[cfg(unix)]
fn service_socket_path(root: &Path) -> std::path::PathBuf {
    let canonical = root.join(SERVICE_SOCKET_RELATIVE_PATH);
    // macOS/BSD SUN_LEN limit is 104 bytes; Linux is 108.
    // When the workspace path is too long, fall back to a short /tmp path
    // derived from a hash of the root to avoid collisions.
    #[cfg(unix)]
    {
        const SUN_PATH_MAX: usize = 104;
        let path_str = canonical.to_string_lossy();
        if path_str.len() >= SUN_PATH_MAX {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            root.hash(&mut hasher);
            let hash = hasher.finish();
            return std::path::PathBuf::from(format!("/tmp/ctox-{hash:x}.sock"));
        }
    }
    canonical
}

fn service_listen_addr(root: &Path) -> String {
    #[cfg(unix)]
    {
        return format!("unix://{}", service_socket_path(root).display());
    }
    #[cfg(not(unix))]
    {
        let host = runtime_env::env_or_config(root, "CTOX_SERVICE_HOST")
            .unwrap_or_else(|| DEFAULT_SERVICE_HOST.to_string());
        let port = runtime_env::env_or_config(root, "CTOX_SERVICE_PORT")
            .unwrap_or_else(|| DEFAULT_SERVICE_PORT.to_string());
        format!("{host}:{port}")
    }
}

#[cfg(unix)]
fn send_service_ipc_request(root: &Path, request: ServiceIpcRequest) -> Result<ServiceIpcResponse> {
    let timeout = service_ipc_timeout(&request);
    let socket_path = service_socket_path(root);
    let mut stream = UnixStream::connect(&socket_path).with_context(|| {
        format!(
            "failed to connect to CTOX service socket {}",
            socket_path.display()
        )
    })?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let mut payload =
        serde_json::to_vec(&request).context("failed to encode CTOX service socket request")?;
    payload.push(b'\n');
    stream.write_all(&payload).with_context(|| {
        format!(
            "failed to write CTOX service socket {}",
            socket_path.display()
        )
    })?;
    stream.flush().with_context(|| {
        format!(
            "failed to flush CTOX service socket {}",
            socket_path.display()
        )
    })?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).with_context(|| {
        format!(
            "failed to read CTOX service socket {}",
            socket_path.display()
        )
    })?;
    if bytes_read == 0 {
        anyhow::bail!("CTOX service socket closed without a response");
    }
    let response: ServiceIpcResponse = serde_json::from_str(line.trim())
        .context("failed to parse CTOX service socket response")?;
    Ok(response)
}

#[cfg(unix)]
fn service_ipc_timeout(request: &ServiceIpcRequest) -> Duration {
    match request {
        ServiceIpcRequest::Status => Duration::from_secs(30),
        ServiceIpcRequest::ScrapeApi { .. } => Duration::from_millis(750),
        ServiceIpcRequest::ChatSubmit { .. } => Duration::from_secs(10),
        ServiceIpcRequest::Stop => Duration::from_secs(2),
    }
}

fn write_pid_file(root: &Path, pid: u32) -> Result<()> {
    let path = service_pid_path(root);
    std::fs::write(&path, format!("{pid}\n"))
        .with_context(|| format!("failed to write service pid file {}", path.display()))
}

fn read_pid_file(root: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(service_pid_path(root)).ok()?;
    raw.trim().parse::<u32>().ok()
}

fn service_pid_path(root: &Path) -> std::path::PathBuf {
    root.join(SERVICE_PID_RELATIVE_PATH)
}

fn service_log_path(root: &Path) -> std::path::PathBuf {
    root.join(SERVICE_LOG_RELATIVE_PATH)
}

fn preferred_ctox_executable(root: &Path) -> Result<std::path::PathBuf> {
    if let Some(bin_dir) =
        runtime_env::env_or_config(root, "CTOX_BIN_DIR").filter(|value| !value.trim().is_empty())
    {
        let candidate = PathBuf::from(bin_dir).join("ctox");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    let current_exe =
        std::env::current_exe().context("failed to resolve current CTOX executable")?;
    Ok(current_exe)
}

fn known_ctox_executable_displays(root: &Path) -> Vec<String> {
    let mut displays = Vec::new();
    if let Ok(current_exe) = std::env::current_exe() {
        displays.push(current_exe.display().to_string());
    }
    if let Some(bin_dir) =
        runtime_env::env_or_config(root, "CTOX_BIN_DIR").filter(|value| !value.trim().is_empty())
    {
        let candidate_display = PathBuf::from(bin_dir).join("ctox").display().to_string();
        if !displays.iter().any(|entry| entry == &candidate_display) {
            displays.push(candidate_display);
        }
    }
    displays
}

fn cleanup_stale_service_runtime(root: &Path) -> Result<()> {
    if let Some(pid) = read_pid_file(root) {
        if !process_is_running(pid) {
            let _ = std::fs::remove_file(service_pid_path(root));
        }
    }
    #[cfg(unix)]
    if matching_service_processes(root, None)?.is_empty() {
        let _ = std::fs::remove_file(service_socket_path(root));
    }
    cleanup_orphan_service_processes(root, None)?;
    supervisor::shutdown_persistent_backends(root)?;
    Ok(())
}

#[cfg(unix)]
fn service_process_matches_root(pid: u32, root: &Path) -> bool {
    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let cwd_path = std::path::PathBuf::from(format!("/proc/{pid}/cwd"));
    if let Ok(cwd) = std::fs::read_link(&cwd_path) {
        let canonical_cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
        if canonical_cwd == canonical_root {
            return true;
        }
    }
    let environ_path = std::path::PathBuf::from(format!("/proc/{pid}/environ"));
    if let Ok(raw) = std::fs::read(&environ_path) {
        for entry in raw.split(|byte| *byte == 0) {
            if let Some(value) = entry.strip_prefix(b"CTOX_ROOT=") {
                let candidate =
                    std::path::PathBuf::from(String::from_utf8_lossy(value).into_owned());
                let canonical_candidate = std::fs::canonicalize(&candidate).unwrap_or(candidate);
                if canonical_candidate == canonical_root {
                    return true;
                }
            }
        }
    }
    false
}

fn matching_service_processes(root: &Path, keep_pid: Option<u32>) -> Result<Vec<u32>> {
    let exe_displays = known_ctox_executable_displays(root);
    let output = Command::new("ps")
        .args(["-axo", "pid=,command="])
        .output()
        .context("failed to inspect running processes")?;
    if !output.status.success() {
        anyhow::bail!("failed to inspect running processes");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut matches = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let Some(pid_raw) = parts.next() else {
            continue;
        };
        let Some(command) = parts.next() else {
            continue;
        };
        let Ok(pid) = pid_raw.trim().parse::<u32>() else {
            continue;
        };
        if Some(pid) == keep_pid || pid == std::process::id() {
            continue;
        }
        if !exe_displays
            .iter()
            .any(|exe_display| command.contains(exe_display))
            || !command.contains("service --foreground")
        {
            continue;
        }
        #[cfg(unix)]
        if !service_process_matches_root(pid, root) {
            continue;
        }
        matches.push(pid);
    }
    matches.sort_unstable();
    matches.dedup();
    Ok(matches)
}

fn cleanup_orphan_service_processes(root: &Path, keep_pid: Option<u32>) -> Result<usize> {
    let mut signaled = 0usize;
    for pid in matching_service_processes(root, keep_pid)? {
        let status = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .with_context(|| format!("failed to signal orphaned CTOX service pid {pid}"))?;
        if !status.success() {
            continue;
        }
        signaled += 1;
        thread::sleep(Duration::from_millis(200));
        if process_is_running(pid) {
            let _ = Command::new("kill")
                .arg("-KILL")
                .arg(pid.to_string())
                .status();
        }
    }
    Ok(signaled)
}

fn service_runtime_idle(root: &Path) -> Result<bool> {
    let pid_idle = read_pid_file(root)
        .map(|pid| !process_is_running(pid))
        .unwrap_or(true);
    let systemd_idle = systemd_unit_status(root)?
        .map(|status| !status.active)
        .unwrap_or(true);
    Ok(pid_idle && systemd_idle && matching_service_processes(root, None)?.is_empty())
}

fn service_shutdown_residue(root: &Path) -> Result<Vec<String>> {
    let mut residue = Vec::new();
    if let Some(pid) = read_pid_file(root) {
        if process_is_running(pid) {
            residue.push(format!("service pid {pid} still alive"));
        } else {
            residue.push(format!(
                "stale service pid file {}",
                service_pid_path(root).display()
            ));
        }
    }
    if let Some(systemd) = systemd_unit_status(root)? {
        if systemd.active {
            residue.push("systemd user unit still active".to_string());
        }
    }
    let service_processes = matching_service_processes(root, None)?;
    if !service_processes.is_empty() {
        residue.push(format!(
            "service foreground processes still alive {service_processes:?}"
        ));
    }
    #[cfg(unix)]
    if service_socket_path(root).exists() && service_processes.is_empty() {
        residue.push(format!(
            "stale service socket {}",
            service_socket_path(root).display()
        ));
    }
    if !supervisor::persistent_backends_idle(root)? {
        residue.push("persistent backends still active".to_string());
    }
    Ok(residue)
}

fn wait_for_service_shutdown(root: &Path, timeout: Duration) -> Result<bool> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if service_runtime_idle(root)? && supervisor::persistent_backends_idle(root)? {
            if let Some(pid) = read_pid_file(root) {
                if !process_is_running(pid) {
                    let _ = std::fs::remove_file(service_pid_path(root));
                }
            }
            #[cfg(unix)]
            {
                let _ = std::fs::remove_file(service_socket_path(root));
            }
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(SERVICE_SHUTDOWN_POLL_MILLIS));
    }
}

fn process_is_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn start_prompt_worker(
    root: std::path::PathBuf,
    state: Arc<Mutex<SharedState>>,
    job: QueuedPrompt,
) {
    thread::spawn(move || {
        match maybe_suppress_fatal_harness_prompt_before_execution(&root, &state, &job) {
            Ok(true) => {
                eprintln!(
                    "ctox prompt worker suppressed-fatal-harness source={} preview={}",
                    job.source_label,
                    clip_text(&job.preview, 120)
                );
                return;
            }
            Ok(false) => {}
            Err(err) => {
                push_event(
                    &state,
                    format!(
                        "Failed to evaluate fatal harness prompt guard for {}: {}",
                        job.source_label, err
                    ),
                );
            }
        }
        if let Some(reason) = crate::service::working_hours::hold_reason(&root) {
            let mut shared = lock_shared_state(&state);
            shared.busy = false;
            shared.current_goal_preview = None;
            shared.active_source_label = None;
            shared.last_progress_epoch_secs = current_epoch_secs();
            insert_pending_prompt_ordered(&mut shared.pending_prompts, job.clone());
            push_event_locked(
                &mut shared,
                format!(
                    "Held {} prompt outside working hours: {}",
                    job.source_label, reason
                ),
            );
            return;
        }
        match maybe_skip_superseded_self_work_prompt(&root, &state, &job) {
            Ok(true) => {
                eprintln!(
                    "ctox prompt worker skip source={} preview={}",
                    job.source_label,
                    clip_text(&job.preview, 120)
                );
                return;
            }
            Ok(false) => {}
            Err(err) => {
                push_event(
                    &state,
                    format!(
                        "Failed to evaluate self-work supersession for {}: {}",
                        job.source_label, err
                    ),
                );
            }
        }
        match maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job) {
            Ok(true) => {
                eprintln!(
                    "ctox prompt worker rerouted-to-strategy source={} preview={}",
                    job.source_label,
                    clip_text(&job.preview, 120)
                );
                return;
            }
            Ok(false) => {}
            Err(err) => {
                push_event(
                    &state,
                    format!(
                        "Failed to evaluate strategic direction routing for {}: {}",
                        job.source_label, err
                    ),
                );
            }
        }
        match maybe_redirect_platform_work_to_expertise_passes(&root, &state, &job) {
            Ok(true) => {
                eprintln!(
                    "ctox prompt worker rerouted source={} preview={}",
                    job.source_label,
                    clip_text(&job.preview, 120)
                );
                return;
            }
            Ok(false) => {}
            Err(err) => {
                push_event(
                    &state,
                    format!(
                        "Failed to evaluate owner-visible platform pass routing for {}: {}",
                        job.source_label, err
                    ),
                );
            }
        }
        eprintln!(
            "ctox prompt worker start source={} preview={}",
            job.source_label,
            clip_text(&job.preview, 120)
        );
        let panic_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db_path = crate::paths::core_db(&root);
            let event_state = state.clone();
            let event_source = job.source_label.clone();
            let workspace_root = job.workspace_root.as_deref().map(std::path::Path::new);
            let conversation_id =
                turn_loop::conversation_id_for_thread_key(job.thread_key.as_deref());
            // Task boundaries — plan-step messages or self-work item
            // closures — must always trigger a continuity refresh, regardless
            // of CTOX_CONTINUITY_REFRESH_EVERY_N_TURNS.
            let force_continuity_refresh = job
                .leased_message_keys
                .iter()
                .any(|key| key.starts_with("plan:system::"));
            let mut execution_prompt = artifact_first_execution_prompt(&job);
            let terminal_bench_preflight = terminal_bench_preflight_spec_for_job(&job);
            match maybe_terminal_bench_controller_runtime_ref_feedback(&root, &job) {
                Ok(Some(note)) => {
                    push_event(
                        &event_state,
                        format!("phase {} terminal-bench-runtime-ref-feedback", event_source),
                    );
                    execution_prompt = format!("{note}\n\n{execution_prompt}");
                }
                Ok(None) => {}
                Err(err) => {
                    let note = format!(
                        "HARNESS FEEDBACK\n\
The harness could not inspect the Terminal-Bench runtime-ticket refs before model execution: {}\n\
Before doing any other work, persist this blocker in controller.json, logbook.md, and run-queue.jsonl with the exact next command needed to repair it. Do not claim completion.",
                        clip_text(&err.to_string(), 600)
                    );
                    push_event(
                        &event_state,
                        format!(
                            "phase {} terminal-bench-runtime-ref-feedback-failed {}",
                            event_source,
                            clip_text(&err.to_string(), 160)
                        ),
                    );
                    execution_prompt = format!("{note}\n\n{execution_prompt}");
                }
            }
            let result = turn_loop::run_chat_turn_with_events_extended_guarded(
                &root,
                &db_path,
                &execution_prompt,
                workspace_root,
                conversation_id,
                job.suggested_skill.as_deref(),
                force_continuity_refresh,
                terminal_bench_preflight,
                false,
                None, // TUI service: per-turn clients (persistent session TODO)
                |event| {
                    push_event(&event_state, format!("phase {} {}", event_source, event));
                },
            );
            let timeout_follow_up_outcome = match &result {
                Err(err) => maybe_enqueue_timeout_continuation(&root, &job, &err.to_string())
                    .ok()
                    .flatten(),
                _ => None,
            };
            let runtime_retry_outcome = match &result {
                Err(err) if timeout_follow_up_outcome.is_none() => {
                    maybe_enqueue_runtime_retry_continuation(&root, &job, &err.to_string())
                        .ok()
                        .flatten()
                }
                _ => None,
            };
            #[test]
            fn outbound_recovery_prompt_gives_agent_exact_reviewed_send_step() {
                let approved_body =
                    "Hallo Julia,\n\ndas ist der freigegebene Text.\n\nViele Gruesse\nINF Yoda";
                let job = QueuedPrompt {
                    prompt: "Schreibe eine Mail an Julia.".to_string(),
                    goal: "send mail".to_string(),
                    preview: "send mail".to_string(),
                    source_label: "tui".to_string(),
                    suggested_skill: None,
                    leased_message_keys: Vec::new(),
                    leased_ticket_event_keys: Vec::new(),
                    thread_key: Some("julia-meeting-notetaker-report-20260505".to_string()),
                    workspace_root: None,
                    ticket_self_work_id: None,
                    outbound_email: Some(channels::FounderOutboundAction {
                        account_key: "email:INF.Yoda@remcapital.de".to_string(),
                        thread_key: "julia-meeting-notetaker-report-20260505".to_string(),
                        subject: "Erste Meeting-Teilnahme als INF Yoda Notetaker".to_string(),
                        to: vec!["j.kienzler@remcapital.de".to_string()],
                        cc: Vec::new(),
                        attachments: Vec::new(),
                    }),
                    outbound_anchor: Some("tui-outbound:test".to_string()),
                };

                let prompt = outcome_witness_recovery_message(
                    Path::new(""),
                    &job,
                    approved_body,
                    "missing artifact",
                );

                assert!(prompt.contains("Die Review-Freigabe"));
                assert!(prompt.contains("Fuehre keine DB- oder Code-Forensik aus"));
                assert!(prompt.contains("BODY=$(cat <<'CTOX_REVIEWED_BODY'"));
                assert!(prompt.contains(approved_body));
                assert!(prompt.contains("ctox channel send --channel email"));
                assert!(prompt.contains("--reviewed-founder-send --body \"$BODY\""));
                assert!(!prompt.contains("<freigegebener Mailtext>"));
            }

            // F3: classify the turn outcome explicitly. The structured value
            // is persisted on the assistant row in `messages.agent_outcome`
            // so downstream consumers (founder-send pipeline, status
            // snapshots) can branch on a typed enum instead of scraping
            // reply text.
            let agent_outcome = match &result {
                Ok(_) => lcm::AgentOutcome::Success,
                Err(err) => classify_agent_failure(&err.to_string()),
            };
            let retryable_runtime_failure = result
                .as_ref()
                .err()
                .map(|err| {
                    let err_text = err.to_string();
                    runtime_error_is_transient_api_failure(&err_text)
                        && !terminal_bench_preflight_retry_loop_should_stop(&job, &err_text)
                })
                .unwrap_or(false);
            // F3: when the turn failed, persist a structured outcome with a
            // neutral, non-leaking body. The legacy "Status: `blocked`" /
            // "Status: `deferred`" prose is no longer how downstream
            // consumers determine the outcome — they read
            // `messages.agent_outcome`. We still record a short neutral
            // body so the conversation transcript stays readable.
            let failure_reply = result.as_ref().err().map(|_err| {
                if timeout_follow_up_outcome.is_some() {
                    "(agent turn deferred to a continuation slice)".to_string()
                } else {
                    "(agent turn did not complete)".to_string()
                }
            });
            if let Some(reply) = &failure_reply {
                let _ =
                    lcm::run_add_assistant_turn(&db_path, conversation_id, reply, agent_outcome);
            }
            // F2: feed the structured outcome into the per-mission
            // agent-failure counter. Successful turns reset the counter;
            // non-success outcomes increment it for status and explicit
            // mission-governance decisions.
            let mut agent_failure_count_after_turn = 0_i64;
            let mut agent_failure_threshold_hit = false;
            if let Ok(engine) = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default()) {
                if agent_outcome.is_agent_failure() && !retryable_runtime_failure {
                    match engine.increment_mission_agent_failure_count(conversation_id) {
                        Ok(record) => {
                            agent_failure_count_after_turn = record.agent_failure_count;
                            let threshold = mission_agent_failure_threshold();
                            if record.agent_failure_count >= threshold {
                                agent_failure_threshold_hit = true;
                                let _ = engine.defer_mission_for_reason(
                                    conversation_id,
                                    "agent_failure_threshold",
                                );
                                let _ = governance::record_event(
                                    &root,
                                    governance::GovernanceEventRequest {
                                        mechanism_id: "agent_failure_threshold",
                                        conversation_id: Some(conversation_id),
                                        severity: "error",
                                        reason: "agent turns repeatedly failed or timed out",
                                        action_taken:
                                            "deferred mission and stopped automatic retry loop",
                                        details: serde_json::json!({
                                            "agent_outcome": agent_outcome.as_str(),
                                            "agent_failure_count": record.agent_failure_count,
                                            "threshold": threshold,
                                            "thread_key": job.thread_key.clone(),
                                            "source_label": job.source_label.clone(),
                                        }),
                                        idempotence_key: Some(&format!(
                                            "agent-failure-threshold:{}:{}",
                                            conversation_id, record.agent_failure_count
                                        )),
                                    },
                                );
                            }
                        }
                        Err(err) => push_event(
                            &state,
                            format!(
                                "agent_failure_count bump failed for conversation {}: {}",
                                conversation_id, err
                            ),
                        ),
                    }
                } else {
                    let _ = engine.reset_mission_agent_failure_count(conversation_id);
                }
            }
            let latest_runtime_error = result.as_ref().err().map(|err| err.to_string());
            let founder_visible_mail_turn =
                is_founder_or_owner_email_job(&job) || job.outbound_email.is_some();
            let context_health = if founder_visible_mail_turn {
                None
            } else {
                assess_current_context_health(&root, &db_path, conversation_id, Some(&job.prompt))
            };
            let mut mission_sync_outcome = if founder_visible_mail_turn {
                None
            } else {
                lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())
                    .and_then(|engine| engine.sync_mission_state_from_continuity(conversation_id))
                    .ok()
            };
            if !founder_visible_mail_turn {
                if let Some(repaired) =
                    run_turn_end_state_invariant_check(&root, &state, conversation_id)
                {
                    mission_sync_outcome = Some(repaired);
                }
            }
            // Completion review gate: when the executor's slice succeeded,
            // hand the slice to a separate, skeptical reviewer agent (a fresh
            // PersistentSession with its own clean context — no executor turn
            // history). The reviewer either ratifies the result (PASS) or
            // CTOX enqueues a rework slice with the reviewer's report as
            // input. Errors / timeouts skip the review (no slice to judge).
            let review_disposition = if completion_review_should_skip_feedback_turn(&job) {
                push_event(
                    &state,
                    format!(
                        "Completion review skipped for {} because feedback turns must not review themselves",
                        job.source_label
                    ),
                );
                CompletionReviewDisposition::None
            } else if let Ok(reply_text) = result.as_ref() {
                push_event(
                    &state,
                    format!(
                        "Completion review start for {} (reply_chars={})",
                        job.source_label,
                        reply_text.chars().count()
                    ),
                );
                let disposition = if let Some(disposition) =
                    short_terminal_bench_artifact_reply_disposition(&root, &job, reply_text)
                {
                    disposition
                } else if let Some(work_id) = continuation_self_work_requested(&job, reply_text) {
                    let summary = format!(
                            "Agentic work is not finished; the last turn explicitly requested continuation with concrete next steps. Last reply: {}",
                            clip_text(reply_text, 260)
                        );
                    CompletionReviewDisposition::ContinueSelfWork {
                        work_id: work_id.to_string(),
                        summary,
                    }
                } else {
                    run_completion_review(
                        &root,
                        &state,
                        &job,
                        reply_text,
                        conversation_id,
                        mission_sync_outcome.as_ref(),
                    )
                };
                push_event(
                    &state,
                    format!(
                        "Completion review disposition for {}: {}",
                        job.source_label,
                        completion_review_disposition_label(&disposition)
                    ),
                );
                disposition
            } else {
                CompletionReviewDisposition::None
            };
            let mut review_requeue: Option<(String, String)> = None;
            let mut outcome_recovery_prompt: Option<QueuedPrompt> = None;
            let mut platform_pipeline_event: Option<String> = None;
            let next_prompt;
            {
                let mut shared = lock_shared_state(&state);
                shared.busy = false;
                shared.current_goal_preview = None;
                shared.active_source_label = None;
                shared.last_completed_at = Some(now_iso_string());
                shared.last_progress_epoch_secs = current_epoch_secs();
                release_leased_keys_locked(
                    &mut shared,
                    &job.leased_message_keys,
                    &job.leased_ticket_event_keys,
                );
                match result {
                    Ok(reply) => {
                        let founder_reply_key =
                            founder_email_reply_message_key(&job).map(ToOwned::to_owned);
                        let proactive_founder_action = if founder_reply_key.is_none() {
                            job.outbound_email.clone()
                        } else {
                            None
                        };
                        let mut founder_send_error: Option<String> = None;
                        let mut should_handle_messages = matches!(
                            &review_disposition,
                            CompletionReviewDisposition::Approved
                                | CompletionReviewDisposition::None
                        );
                        let terminal_no_send = matches!(
                            &review_disposition,
                            CompletionReviewDisposition::NoSend { .. }
                        );
                        let held_benchmark_controller = matches!(
                            &review_disposition,
                            CompletionReviewDisposition::Hold { .. }
                        )
                            && is_terminal_bench_controller_artifact_job(&job);
                        let expected_artifact_refs = expected_outcome_artifacts_for_job(&job);
                        let delivered_artifact_refs = match delivered_outcome_artifacts_for_job(
                            &root,
                            &job,
                            &expected_artifact_refs,
                        ) {
                            Ok(refs) => refs,
                            Err(err) => {
                                founder_send_error = Some(format!(
                                    "Der Ergebnisnachweis konnte nicht gelesen werden: {}",
                                    err
                                ));
                                Vec::new()
                            }
                        };
                        if !expected_artifact_refs.is_empty() {
                            push_event_locked(
                                &mut shared,
                                format!(
                                    "Outcome witness checking {} expected artifact(s), {} delivered artifact(s) for {}",
                                    expected_artifact_refs.len(),
                                    delivered_artifact_refs.len(),
                                    job_outcome_entity_id(&job)
                                ),
                            );
                        }
                        let outcome_witness_allowed = matches!(
                            &review_disposition,
                            CompletionReviewDisposition::Approved
                                | CompletionReviewDisposition::None
                        );
                        let mut outcome_witness_proof_id: Option<String> = None;
                        let mut outcome_witness_error: Option<String> = None;
                        if !terminal_no_send && outcome_witness_allowed {
                            match enforce_job_outcome_witness(
                                &root,
                                &job,
                                expected_artifact_refs.clone(),
                                delivered_artifact_refs.clone(),
                            ) {
                                Ok(proof_id) => {
                                    outcome_witness_proof_id = proof_id;
                                }
                                Err(err) => {
                                    outcome_witness_error = Some(err.to_string());
                                }
                            }
                        }
                        if !expected_artifact_refs.is_empty() && outcome_witness_error.is_none() {
                            if let Some(proof_id) = outcome_witness_proof_id.as_deref() {
                                push_event_locked(
                                    &mut shared,
                                    format!(
                                        "Outcome witness accepted proof {} for {}",
                                        proof_id,
                                        job_outcome_entity_id(&job)
                                    ),
                                );
                            } else if !terminal_no_send && outcome_witness_allowed {
                                outcome_witness_error = Some(format!(
                                    "Harness invariant violation: {} expected durable outcome artifact(s), but no core transition proof was recorded for {}.",
                                    expected_artifact_refs.len(),
                                    job_outcome_entity_id(&job)
                                ));
                            }
                        }
                        if let Some(err) = outcome_witness_error.as_ref() {
                            push_event_locked(
                                &mut shared,
                                format!(
                                    "Outcome witness rejected {}: {}",
                                    job_outcome_entity_id(&job),
                                    clip_text(err, 220)
                                ),
                            );
                            should_handle_messages = false;
                            if founder_send_error.is_none() {
                                founder_send_error = Some(outcome_witness_recovery_message(
                                    &root, &job, &reply, err,
                                ));
                            }
                            if job.ticket_self_work_id.is_none()
                                && job.leased_message_keys.is_empty()
                                && job.outbound_email.is_some()
                                && outcome_witness_rejection_count(&root, &job)
                                    .map(|count| {
                                        count < review_checkpoint_requeue_block_threshold()
                                    })
                                    .unwrap_or(true)
                            {
                                let recovery =
                                    founder_send_error.as_ref().cloned().unwrap_or_else(|| {
                                        outcome_witness_recovery_message(&root, &job, &reply, err)
                                    });
                                outcome_recovery_prompt = Some(QueuedPrompt {
                                    prompt: recovery.clone(),
                                    goal: format!(
                                        "Complete reviewed send for {}",
                                        job.source_label
                                    ),
                                    preview: clip_text(&recovery, 180),
                                    source_label: "outcome-witness-recovery".to_string(),
                                    suggested_skill: job.suggested_skill.clone(),
                                    leased_message_keys: Vec::new(),
                                    leased_ticket_event_keys: Vec::new(),
                                    thread_key: job.thread_key.clone(),
                                    workspace_root: job.workspace_root.clone(),
                                    ticket_self_work_id: None,
                                    outbound_email: job.outbound_email.clone(),
                                    outbound_anchor: job.outbound_anchor.clone(),
                                });
                            }
                            if job.ticket_self_work_id.is_none()
                                && !job.leased_message_keys.is_empty()
                                && job.outbound_email.is_none()
                                && should_queue_artifact_outcome_recovery(&job)
                                && outcome_witness_rejection_count(&root, &job)
                                    .map(|count| {
                                        count < review_checkpoint_requeue_block_threshold()
                                    })
                                    .unwrap_or(true)
                            {
                                let recovery =
                                    founder_send_error.as_ref().cloned().unwrap_or_else(|| {
                                        outcome_witness_recovery_message(&root, &job, &reply, err)
                                    });
                                outcome_recovery_prompt = Some(QueuedPrompt {
                                    prompt: recovery.clone(),
                                    goal: format!("Complete required artifacts for {}", job.goal),
                                    preview: clip_text(&recovery, 180),
                                    source_label: "outcome-witness-recovery".to_string(),
                                    suggested_skill: job.suggested_skill.clone(),
                                    leased_message_keys: job.leased_message_keys.clone(),
                                    leased_ticket_event_keys: job.leased_ticket_event_keys.clone(),
                                    thread_key: job.thread_key.clone(),
                                    workspace_root: job.workspace_root.clone(),
                                    ticket_self_work_id: None,
                                    outbound_email: None,
                                    outbound_anchor: job.outbound_anchor.clone(),
                                });
                            }
                        }
                        if founder_send_error.is_none() && !terminal_no_send {
                            if let Some(message_key) = founder_reply_key.as_deref() {
                                let _ = close_open_founder_communication_self_work_for_inbound(
                                    &root,
                                    message_key,
                                    "Founder communication completed after reviewed outbound send.",
                                );
                            }
                        }
                        if !job.leased_message_keys.is_empty() && should_handle_messages {
                            let _ = channels::ack_leased_messages(
                                &root,
                                &job.leased_message_keys,
                                "handled",
                            );
                            // Auto-complete plan steps whose emit message was
                            // just handled by this turn so the plan advances
                            // without the model needing to call complete-step.
                            for key in &job.leased_message_keys {
                                if key.starts_with("plan:system::") {
                                    let _ = plan::complete_step_by_message_key(&root, key, &reply);
                                }
                            }
                        } else if !job.leased_message_keys.is_empty() && terminal_no_send {
                            let _ = channels::ack_leased_messages(
                                &root,
                                &job.leased_message_keys,
                                "cancelled",
                            );
                        } else if !job.leased_message_keys.is_empty() {
                            let held_founder_review = founder_reply_key.is_some()
                                || proactive_founder_action.is_some()
                                || is_founder_or_owner_email_job(&job);
                            let retry_status = if outcome_witness_error.is_some() {
                                outcome_witness_retry_route_status_for_job(&root, &job)
                            } else if held_founder_review || held_benchmark_controller {
                                "review_rework"
                            } else {
                                "pending"
                            };
                            let _ = channels::ack_leased_messages(
                                &root,
                                &job.leased_message_keys,
                                retry_status,
                            );
                        }
                        if !job.leased_ticket_event_keys.is_empty() {
                            let _ = tickets::ack_leased_ticket_events(
                                &root,
                                &job.leased_ticket_event_keys,
                                "handled",
                            );
                        }
                        shared.last_error = founder_send_error.clone();
                        shared.last_reply_chars = Some(reply.chars().count());
                        if let Some(work_id) = job.ticket_self_work_id.as_deref() {
                            match &review_disposition {
                                CompletionReviewDisposition::RequeueSelfWork {
                                    work_id: target_work_id,
                                    summary,
                                } => {
                                    review_requeue =
                                        Some((target_work_id.clone(), summary.clone()));
                                    push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Review rejected the slice; preserving durable self-work {} instead of closing it",
                                            target_work_id
                                        ),
                                    );
                                }
                                CompletionReviewDisposition::ContinueSelfWork {
                                    work_id: target_work_id,
                                    summary,
                                } => match requeue_continue_requested_self_work(
                                    &root,
                                    target_work_id,
                                    summary,
                                ) {
                                    Ok(Some(queued)) => push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Agent requested continuation; requeued durable self-work {} via {}",
                                            target_work_id, queued.title
                                        ),
                                    ),
                                    Ok(None) => push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Agent requested continuation for {}, but no queue item was created",
                                            target_work_id
                                        ),
                                    ),
                                    Err(err) => push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Agent continuation requeue failed for {}: {}",
                                            target_work_id, err
                                        ),
                                    ),
                                },
                                CompletionReviewDisposition::Approved
                                | CompletionReviewDisposition::None => {
                                    if outcome_witness_error.is_none() && founder_send_error.is_none() {
                                        let note = format!(
                                            "Execution slice completed successfully. Reply summary: {}",
                                            clip_text(&reply, 220)
                                        );
                                        close_ticket_self_work_item(&root, work_id, &note);
                                        platform_pipeline_event =
                                            maybe_continue_platform_expertise_pipeline_after_success(
                                                &root, &job,
                                            )
                                            .ok()
                                            .flatten();
                                    } else if let Some(err) = outcome_witness_error.as_ref() {
                                        let recovery = founder_send_error
                                            .as_ref()
                                            .cloned()
                                            .unwrap_or_else(|| {
                                                outcome_witness_recovery_message(&root, &job, &reply, err)
                                            });
                                        review_requeue = Some((work_id.to_string(), recovery));
                                        push_event_locked(
                                            &mut shared,
                                            format!(
                                                "Outcome witness rejected closure for {}; preserving durable self-work {}",
                                                job.source_label, work_id
                                            ),
                                        );
                                    }
                                }
                                CompletionReviewDisposition::NoSend { summary } => {
                                    if founder_send_error.is_none() {
                                        close_ticket_self_work_item(
                                            &root,
                                            work_id,
                                            &format!(
                                                "Founder communication closed without sending after review: {}",
                                                clip_text(summary, 220)
                                            ),
                                        );
                                    }
                                }
                                CompletionReviewDisposition::Hold { summary } => {
                                    push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Review held the slice open without send/closure: {}",
                                            clip_text(summary, 180)
                                        ),
                                    );
                                }
                                CompletionReviewDisposition::RewriteOnly {
                                    findings,
                                    review_summary,
                                    ..
                                } => {
                                    push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Review found {} rewrite-class issue(s); body fix scheduled in-process: {}",
                                            findings.len(),
                                            clip_text(review_summary, 180)
                                        ),
                                    );
                                }
                                CompletionReviewDisposition::FeedbackRetry {
                                    review_summary,
                                    ..
                                } => {
                                    push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Review fed back substantive guidance without spawning rework: {}",
                                            clip_text(review_summary, 180)
                                        ),
                                    );
                                }
                            }
                        }
                        push_event_locked(
                            &mut shared,
                            if let Some(err) = founder_send_error {
                                format!(
                                    "Completed {} draft with {} chars but founder send stayed pending: {}",
                                    job.source_label,
                                    reply.chars().count(),
                                    clip_text(&err, 180)
                                )
                            } else {
                                format!(
                                    "Completed {} reply with {} chars",
                                    job.source_label,
                                    reply.chars().count()
                                )
                            },
                        );
                    }
                    Err(err) => {
                        let err_text = err.to_string();
                        let compact_error = turn_loop::summarize_runtime_error(&err_text);
                        let terminal_bench_preflight_loop_stop =
                            terminal_bench_preflight_retry_loop_should_stop(&job, &err_text);
                        let retry_founder_message =
                            founder_email_worker_error_is_retryable(&job, &err_text);
                        let retry_runtime_message =
                            runtime_error_is_transient_api_failure(&err_text)
                                && !terminal_bench_preflight_loop_stop;
                        let timeout_worker_message =
                            matches!(agent_outcome, lcm::AgentOutcome::TurnTimeout);
                        let timeout_retry_message =
                            timeout_worker_message && timeout_auto_retry_enabled();
                        let retry_worker_message =
                            retry_founder_message || retry_runtime_message || timeout_retry_message;
                        let retry_has_durable_resume = !job.leased_message_keys.is_empty()
                            || job.ticket_self_work_id.is_some()
                            || timeout_follow_up_outcome.is_some()
                            || runtime_retry_outcome.is_some();
                        let retry_not_before = if retry_runtime_message {
                            Some(runtime_retry_not_before_iso(&err_text))
                        } else if timeout_retry_message && !agent_failure_threshold_hit {
                            Some(timeout_retry_not_before_iso(agent_failure_count_after_turn))
                        } else {
                            None
                        };
                        if !job.leased_message_keys.is_empty() {
                            if retry_runtime_message {
                                match apply_runtime_retry_feedback_to_leased_queue(
                                    &root, &job, &err_text,
                                ) {
                                    Ok(updated) if updated > 0 => push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Injected harness retry feedback into {updated} queued message(s)"
                                        ),
                                    ),
                                    Ok(_) => {}
                                    Err(update_err) => push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Failed to inject harness retry feedback into queued message: {}",
                                            clip_text(&update_err.to_string(), 180)
                                        ),
                                    ),
                                }
                            }
                            if terminal_bench_preflight_loop_stop {
                                let note = terminal_bench_preflight_loop_stop_note(&err_text);
                                for message_key in &job.leased_message_keys {
                                    let _ = channels::update_queue_task(
                                        &root,
                                        channels::QueueTaskUpdateRequest {
                                            message_key: message_key.clone(),
                                            status_note: Some(note.clone()),
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                            let route_status = if terminal_bench_preflight_loop_stop {
                                "blocked"
                            } else {
                                failed_worker_route_status(
                                    agent_failure_threshold_hit,
                                    timeout_worker_message,
                                    retry_worker_message,
                                )
                            };
                            if let Some(not_before) = retry_not_before.as_deref() {
                                let _ = channels::defer_messages_until(
                                    &root,
                                    &job.leased_message_keys,
                                    not_before,
                                    if timeout_worker_message {
                                        "turn timeout retry backoff"
                                    } else {
                                        "retryable runtime/API failure"
                                    },
                                );
                            }
                            let _ = channels::ack_leased_messages(
                                &root,
                                &job.leased_message_keys,
                                route_status,
                            );
                        }
                        if !job.leased_ticket_event_keys.is_empty() {
                            let _ = tickets::ack_leased_ticket_events(
                                &root,
                                &job.leased_ticket_event_keys,
                                "failed",
                            );
                        }
                        shared.last_reply_chars =
                            failure_reply.as_ref().map(|reply| reply.chars().count());
                        shared.last_error = Some(compact_error.clone());
                        if let Some(work_id) = job.ticket_self_work_id.as_deref() {
                            if agent_failure_threshold_hit {
                                let note = format!(
                                    "Execution slice repeatedly failed or timed out. The mission was deferred by the agent-failure threshold and automatic retries were stopped. Last error: {}",
                                    compact_error
                                );
                                block_ticket_self_work_item(&root, work_id, &note);
                            } else if timeout_worker_message && !timeout_retry_message {
                                let note = format!(
                                    "Execution slice hit the turn time budget. Automatic same-prompt retry was blocked because repeating an interrupted multi-turn can restart work and burn tokens. Resume requires a compacted continuation checkpoint or an explicit operator retry. Error: {}",
                                    compact_error
                                );
                                block_ticket_self_work_item(&root, work_id, &note);
                            } else if retry_worker_message {
                                let note = format!(
                                    "The agent run was interrupted and this same work item was left pending for retry without spawning a continuation task. The agent must resume the original task, perform the required action itself, and only finish after the durable outcome exists. Error: {}",
                                    compact_error
                                );
                                match requeue_runtime_failed_self_work(&root, work_id, &note) {
                                    Ok(Some(queued)) => push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Retryable worker error; requeued durable self-work {} via {}",
                                            work_id, queued.title
                                        ),
                                    ),
                                    Ok(None) => push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Retryable worker error; kept durable self-work {} queued/pending",
                                            work_id
                                        ),
                                    ),
                                    Err(requeue_err) => {
                                        push_event_locked(
                                            &mut shared,
                                            format!(
                                                "Retryable worker error; failed to requeue durable self-work {}: {}",
                                                work_id, requeue_err
                                            ),
                                        );
                                        let _ = tickets::append_ticket_self_work_note(
                                            &root,
                                            work_id,
                                            &note,
                                            "ctox-service",
                                            "internal",
                                        );
                                    }
                                }
                            } else if let Some(title) = &timeout_follow_up_outcome {
                                let note = format!(
                                    "Execution slice hit the turn time budget. Durable continuation: {}",
                                    title
                                );
                                supersede_ticket_self_work_item(&root, work_id, &note);
                            } else {
                                let note = format!("Execution slice failed: {}", compact_error);
                                block_ticket_self_work_item(&root, work_id, &note);
                            }
                        }
                        if terminal_bench_preflight_loop_stop {
                            push_event_locked(
                                &mut shared,
                                format!(
                                    "{} prompt hit repeated Terminal-Bench preflight violations; automatic retry stopped: {compact_error}",
                                    job.source_label
                                ),
                            );
                        } else if retry_worker_message && retry_has_durable_resume {
                            push_event_locked(
                                &mut shared,
                                format!(
                                    "{} prompt hit a retryable runtime error and will retry: {compact_error}",
                                    job.source_label
                                ),
                            );
                        } else if retry_worker_message {
                            push_event_locked(
                                &mut shared,
                                format!(
                                    "{} prompt hit a retryable runtime error; automatic standalone retry was suppressed: {compact_error}",
                                    job.source_label
                                ),
                            );
                        } else if timeout_worker_message {
                            push_event_locked(
                                &mut shared,
                                format!(
                                    "{} prompt hit the turn time budget and automatic same-prompt retry was blocked: {compact_error}",
                                    job.source_label
                                ),
                            );
                        } else {
                            push_event_locked(
                                &mut shared,
                                format!("{} prompt failed: {compact_error}", job.source_label),
                            );
                        }
                        if let Some(title) = &timeout_follow_up_outcome {
                            push_event_locked(
                                &mut shared,
                                format!("Created timeout continuation task: {title}"),
                            );
                        }
                        if let Some(title) = &runtime_retry_outcome {
                            push_event_locked(
                                &mut shared,
                                format!("Created runtime retry task after transient API failure: {title}"),
                            );
                        }
                    }
                }
                if let Some(health) = &context_health {
                    push_event_locked(
                        &mut shared,
                        format!(
                            "Context health {} ({})",
                            health.overall_score,
                            health.status.as_str()
                        ),
                    );
                }
                if let Some(mission) = &mission_sync_outcome {
                    push_event_locked(
                        &mut shared,
                        format!(
                            "Mission sync {} ({})",
                            if mission.is_open { "open" } else { "closed" },
                            mission.continuation_mode
                        ),
                    );
                }
                if let Some(event) = &platform_pipeline_event {
                    push_event_locked(&mut shared, event.clone());
                }
                // Lightweight rewrite path: when every reviewer finding is
                // structurally `rewrite`-class, synthesise an in-process
                // body-fix prompt instead of spawning the heavy rework
                // queue task. The prompt inherits outbound recipient/anchor
                // metadata from the parent job, sandwiches the prior body
                // and the categorized findings, and is pushed to the front
                // of the pending queue so the next pick picks it up.
                if let CompletionReviewDisposition::RewriteOnly {
                    findings,
                    prior_body,
                    anchor_message_key,
                    review_summary,
                } = &review_disposition
                {
                    let synthesised = synthesise_review_rewrite_prompt(
                        &job,
                        findings,
                        prior_body,
                        anchor_message_key.as_deref(),
                        review_summary,
                    );
                    shared.pending_prompts.push_front(synthesised);
                    push_event_locked(
                        &mut shared,
                        format!(
                            "Queued lightweight rewrite-only retry ({} finding(s)) for {}",
                            findings.len(),
                            job.source_label
                        ),
                    );
                    if let Ok(engine) = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default()) {
                        match engine.increment_mission_rewrite_failure_count(conversation_id) {
                            Ok(record) => {
                                let threshold = mission_rewrite_failure_threshold();
                                if record.rewrite_failure_count >= threshold {
                                    let _ = engine.defer_mission_for_reason(
                                        conversation_id,
                                        "rewrite_failure_threshold",
                                    );
                                    push_event_locked(
                                        &mut shared,
                                        format!(
                                            "Rewrite-only loop hit threshold ({}) for conversation {}; mission deferred",
                                            threshold, conversation_id
                                        ),
                                    );
                                    // Drop the synthesised retry — the
                                    // mission is now deferred and we do
                                    // not want to keep re-spawning. Pop
                                    // the prompt we just pushed to the
                                    // front.
                                    let _ = shared.pending_prompts.pop_front();
                                    let _ = governance::record_event(
                                        &root,
                                        governance::GovernanceEventRequest {
                                            mechanism_id: "review_rewrite_threshold",
                                            conversation_id: Some(conversation_id),
                                            severity: "warning",
                                            reason: "rewrite-only review iterations failed to converge",
                                            action_taken: "deferred mission and stopped respawning rewrite retries",
                                            details: serde_json::json!({
                                                "thread_key": job.thread_key,
                                                "source_label": job.source_label,
                                                "rewrite_failure_count": record.rewrite_failure_count,
                                                "threshold": threshold,
                                            }),
                                            idempotence_key: Some(&format!(
                                                "rewrite-threshold:{}:{}",
                                                conversation_id, record.rewrite_failure_count
                                            )),
                                        },
                                    );
                                }
                            }
                            Err(err) => push_event_locked(
                                &mut shared,
                                format!(
                                    "rewrite_failure_count bump failed for conversation {}: {}",
                                    conversation_id, err
                                ),
                            ),
                        }
                    }
                } else if let CompletionReviewDisposition::FeedbackRetry {
                    feedback_prompt,
                    review_summary,
                } = &review_disposition
                {
                    shared.pending_prompts.push_front(QueuedPrompt {
                        prompt: feedback_prompt.clone(),
                        goal: format!("Address review feedback for {}", job.goal),
                        preview: clip_text(review_summary, 180),
                        source_label: "review-feedback".to_string(),
                        suggested_skill: job.suggested_skill.clone(),
                        leased_message_keys: Vec::new(),
                        leased_ticket_event_keys: Vec::new(),
                        thread_key: job.thread_key.clone(),
                        workspace_root: job.workspace_root.clone(),
                        ticket_self_work_id: None,
                        outbound_email: job.outbound_email.clone(),
                        outbound_anchor: job.outbound_anchor.clone(),
                    });
                    push_event_locked(
                        &mut shared,
                        format!(
                            "Queued in-process review feedback retry for {}",
                            job.source_label
                        ),
                    );
                } else if let CompletionReviewDisposition::Hold { summary } = &review_disposition {
                    if job.ticket_self_work_id.is_none()
                        && job.outbound_email.is_none()
                        && is_terminal_bench_controller_artifact_job(&job)
                    {
                        let feedback_prompt =
                            terminal_bench_controller_hold_feedback_prompt(&job, summary);
                        shared.pending_prompts.push_front(QueuedPrompt {
                            prompt: feedback_prompt.clone(),
                            goal: format!("Continue Terminal-Bench controller for {}", job.goal),
                            preview: clip_text(&feedback_prompt, 180),
                            source_label: "review-feedback".to_string(),
                            suggested_skill: job.suggested_skill.clone(),
                            leased_message_keys: job.leased_message_keys.clone(),
                            leased_ticket_event_keys: Vec::new(),
                            thread_key: job.thread_key.clone(),
                            workspace_root: job.workspace_root.clone(),
                            ticket_self_work_id: None,
                            outbound_email: None,
                            outbound_anchor: job.outbound_anchor.clone(),
                        });
                        push_event_locked(
                            &mut shared,
                            format!(
                                "Queued benchmark controller continuation feedback for {} after review hold",
                                job.source_label
                            ),
                        );
                    }
                } else if matches!(&review_disposition, CompletionReviewDisposition::Approved) {
                    // Successful approval clears the rewrite-only failure
                    // counter so a future regression starts from zero.
                    if let Ok(engine) = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default()) {
                        let _ = engine.reset_mission_rewrite_failure_count(conversation_id);
                    }
                }
                next_prompt = maybe_start_next_queued_prompt_after_recovery_locked(
                    &root,
                    &mut shared,
                    outcome_recovery_prompt.is_some(),
                );
            }
            if let Some((work_id, summary)) = review_requeue {
                match requeue_review_rejected_self_work(&root, &work_id, &summary) {
                    Ok(Some(task)) => push_event(
                        &state,
                        format!(
                            "Review rejected the slice; re-queued durable self-work {} as {}",
                            work_id, task.title
                        ),
                    ),
                    Ok(None) => push_event(
                        &state,
                        format!(
                            "Review rejected the slice; durable self-work {} was kept queued without creating a duplicate runnable task",
                            work_id
                        ),
                    ),
                    Err(err) => push_event(
                        &state,
                        format!(
                            "Failed to re-queue durable self-work {} after review rejection: {}",
                            work_id, err
                        ),
                    ),
                }
            }
            let queued_outcome_recovery = outcome_recovery_prompt.is_some();
            if let Some(queued) = outcome_recovery_prompt {
                enqueue_prompt(
                    &root,
                    &state,
                    queued,
                    "Queued outcome-witness recovery for reviewed send".to_string(),
                );
            }
            if !queued_outcome_recovery {
                if let Some(queued) = next_prompt {
                    start_prompt_worker(root.clone(), state.clone(), queued);
                }
            }
            match &latest_runtime_error {
                Some(error) => eprintln!(
                    "ctox prompt worker end source={} error={}",
                    job.source_label,
                    turn_loop::summarize_runtime_error(error)
                ),
                None => eprintln!("ctox prompt worker end source={} ok", job.source_label),
            }
        }));
        if panic_outcome.is_err() {
            let mut next_prompt = None;
            {
                let mut shared = lock_shared_state(&state);
                shared.busy = false;
                shared.current_goal_preview = None;
                shared.active_source_label = None;
                shared.last_completed_at = Some(now_iso_string());
                shared.last_progress_epoch_secs = current_epoch_secs();
                shared.last_reply_chars = None;
                shared.last_error = Some(
                    "CTOX prompt worker panicked before the turn could finish. See service log."
                        .to_string(),
                );
                release_leased_keys_locked(
                    &mut shared,
                    &job.leased_message_keys,
                    &job.leased_ticket_event_keys,
                );
                if let Some(work_id) = job.ticket_self_work_id.as_deref() {
                    block_ticket_self_work_item(
                        &root,
                        work_id,
                        "Execution worker panicked before the slice could finish. Inspect the service log.",
                    );
                }
                push_event_locked(
                    &mut shared,
                    format!("{} prompt panicked before cleanup", job.source_label),
                );
                if let Some(remaining_secs) = runtime_blocker_backoff_remaining_secs(&shared) {
                    if !shared.pending_prompts.is_empty() {
                        push_event_locked(
                            &mut shared,
                            format!(
                                "Deferred queued prompt dispatch for {}s due to hard runtime blocker",
                                remaining_secs
                            ),
                        );
                    }
                } else {
                    next_prompt = maybe_start_next_queued_prompt_locked(&root, &mut shared);
                }
            }
            if let Some(queued) = next_prompt {
                start_prompt_worker(root, state, queued);
            }
            eprintln!("ctox prompt worker end source={} panic", job.source_label);
        }
    });
}

/// Hand the just-completed slice to a separate completion-reviewer agent.
///
/// The reviewer runs in a fresh `PersistentSession` (its own clean codex-core
/// thread, no executor turn history) with a skeptical, scope-bound system
/// prompt. It either ratifies the slice (PASS) or surfaces concrete
/// objections (FAIL/PARTIAL). On rejection the reviewer's report is enqueued
/// as a high-priority rework slice on the same thread — the original ack
/// path is unchanged so the user still sees the executor's reply.
///
/// Failures inside the review path (LCM open errors, gateway timeouts) are
/// swallowed and surfaced as events: the slice falls through unjudged rather
/// than blocking the worker.
fn run_completion_review(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    job: &QueuedPrompt,
    reply_text: &str,
    conversation_id: i64,
    _mission_state: Option<&lcm::MissionStateRecord>,
) -> CompletionReviewDisposition {
    let owner_visible = derive_owner_visible_for_review(&job.source_label);
    let db_path = crate::paths::core_db(&root);
    let review_skill_path = root
        .join("skills/system/review/external-review/SKILL.md")
        .to_string_lossy()
        .to_string();
    let founder_reply_key = founder_email_reply_message_key(job);
    let founder_reply_action = founder_reply_key
        .and_then(|message_key| channels::prepare_reviewed_founder_reply(root, message_key).ok());
    let proactive_founder_action = if founder_reply_key.is_none() {
        job.outbound_email.clone()
    } else {
        None
    };
    let founder_required_deliverables = founder_reply_key
        .and_then(|message_key| {
            channels::required_founder_reply_deliverables(root, message_key).ok()
        })
        .unwrap_or_default();
    let founder_commitments =
        if is_founder_or_owner_email_job(job) || proactive_founder_action.is_some() {
            detect_founder_mail_commitments(reply_text)
        } else {
            Vec::new()
        };
    let founder_commitment_backing = if founder_commitments.is_empty() {
        Vec::new()
    } else {
        founder_commitment_backing_summaries(root)
    };
    let review_request = review::CompletionReviewRequest {
        preview: job.preview.clone(),
        source_label: job.source_label.clone(),
        owner_visible,
        conversation_id,
        thread_key: job.thread_key.clone().unwrap_or_default(),
        workspace_root: job.workspace_root.clone().unwrap_or_default(),
        runtime_db_path: db_path.to_string_lossy().to_string(),
        review_skill_path,
        artifact_text: reply_text.to_string(),
        artifact_action: founder_reply_action
            .as_ref()
            .map(|_| "reply".to_string())
            .or_else(|| {
                proactive_founder_action
                    .as_ref()
                    .map(|_| "proactive_founder_outbound_email".to_string())
            }),
        artifact_to: founder_reply_action
            .as_ref()
            .map(|action| action.to.clone())
            .or_else(|| {
                proactive_founder_action
                    .as_ref()
                    .map(|action| action.to.clone())
            })
            .unwrap_or_default(),
        artifact_cc: founder_reply_action
            .as_ref()
            .map(|action| action.cc.clone())
            .or_else(|| {
                proactive_founder_action
                    .as_ref()
                    .map(|action| action.cc.clone())
            })
            .unwrap_or_default(),
        artifact_attachments: founder_reply_action
            .as_ref()
            .map(|action| action.attachments.clone())
            .or_else(|| {
                proactive_founder_action
                    .as_ref()
                    .map(|action| action.attachments.clone())
            })
            .unwrap_or_default(),
        required_deliverables: founder_required_deliverables,
        artifact_commitments: founder_commitments.clone(),
        commitment_backing: founder_commitment_backing.clone(),
    };
    let mut outcome = review::review_completion_if_needed(root, &review_request, reply_text);
    if is_founder_or_owner_email_job(job) || proactive_founder_action.is_some() {
        if let Some(guard_outcome) = founder_commitment_guard_outcome(
            &review_request.artifact_commitments,
            &review_request.commitment_backing,
        ) {
            push_event(
                state,
                format!(
                    "Founder communication guard blocked unbacked commitment(s): {}",
                    clip_text(&guard_outcome.summary, 180)
                ),
            );
            outcome = guard_outcome;
        }
    }
    if !outcome.required {
        // Founder-visible mail is never allowed to fall through unreviewed.
        // If the gate declines to run, hold the outbound path and force
        // explicit rework instead of sending or immediately retrying.
        if is_founder_or_owner_email_job(job) || proactive_founder_action.is_some() {
            let summary =
                "Founder communication was held because no completion review was produced.";
            push_event(state, summary.to_string());
            return CompletionReviewDisposition::Hold {
                summary: summary.to_string(),
            };
        }
        // Heuristic decided this slice does not need review — stay quiet.
        return CompletionReviewDisposition::None;
    }
    let verification_request = verification::SliceVerificationRequest {
        conversation_id,
        goal: job.goal.clone(),
        prompt: job.prompt.clone(),
        preview: review_request.preview.clone(),
        source_label: review_request.source_label.clone(),
        owner_visible,
    };
    if let Err(err) = verification::record_slice_assurance(
        root,
        &verification_request,
        reply_text,
        None,
        Some(&outcome),
    ) {
        push_event(
            state,
            format!(
                "Completion review persist failed for {}: {}",
                job.source_label, err
            ),
        );
    }
    push_event(
        state,
        format!(
            "Completion review {} for {} (score={}): {}",
            outcome.verdict.as_gate_label(),
            job.source_label,
            outcome.score,
            clip_text(&outcome.summary, 160),
        ),
    );
    // Only enqueue a rework slice for actionable verdicts (FAIL / PARTIAL).
    // `Unavailable` means the reviewer itself failed (timeout, gateway error)
    // — the executor's work might be fine; we surface it but do not auto-rework
    // on a flaky reviewer.
    let actionable_rejection = outcome.requires_follow_up()
        && !matches!(outcome.verdict, review::ReviewVerdict::Unavailable);
    // Structural routing class derived from the reviewer's CATEGORIZED_FINDINGS
    // block. Empty findings ⇒ Approved; all `rewrite` ⇒ RewriteOnly; any
    // `rework` (or mixed) ⇒ Substantive. Legacy reports without a
    // categorized block fall through to Substantive because
    // `categorized_findings` will be empty *and* the reviewer's verdict
    // dictates the path — `Approved` for Pass, the heavy path for Fail.
    let routing_class = classify_findings(&outcome.categorized_findings);
    let founder_mail_source = matches!(
        job.source_label.to_ascii_lowercase().as_str(),
        "email:owner" | "email:founder" | "email:admin"
    );
    if founder_mail_source {
        return match outcome.verdict {
            review::ReviewVerdict::Pass => {
                if let Some(message_key) = founder_reply_key {
                    if let Err(err) = channels::record_founder_reply_review_approval(
                        root,
                        message_key,
                        reply_text,
                        &outcome.summary,
                    ) {
                        push_event(
                            state,
                            format!(
                                "Founder review passed for {} but approval persistence failed: {}",
                                job.source_label, err
                            ),
                        );
                        return CompletionReviewDisposition::Hold {
                            summary: err.to_string(),
                        };
                    }
                }
                CompletionReviewDisposition::Approved
            }
            review::ReviewVerdict::Fail if actionable_rejection => {
                if review_outcome_is_terminal_no_send(&outcome) {
                    push_event(
                        state,
                        format!(
                            "Founder review closed {} without sending because the correct action is to wait: {}",
                            job.source_label,
                            clip_text(&outcome.summary, 180)
                        ),
                    );
                    return CompletionReviewDisposition::NoSend {
                        summary: outcome.summary.clone(),
                    };
                }
                if let Some(disposition) = no_cascade_review_block(root, job, &outcome) {
                    return disposition;
                }
                if matches!(routing_class, ReviewRoutingClass::Stale) {
                    match enqueue_review_stale_refresh(root, job, &outcome) {
                        Ok(title) => push_event(
                            state,
                            format!("Founder review stale refresh enqueued: {title}"),
                        ),
                        Err(err) => push_event(
                            state,
                            format!(
                                "Founder review stale refresh enqueue failed for {}: {}",
                                job.source_label, err
                            ),
                        ),
                    }
                    return CompletionReviewDisposition::Hold {
                        summary: outcome.summary.clone(),
                    };
                }
                if matches!(routing_class, ReviewRoutingClass::RewriteOnly) {
                    let findings = rewrite_findings_from(&outcome.categorized_findings);
                    push_event(
                        state,
                        format!(
                            "Founder review fail for {} routed to lightweight rewrite-only path ({} finding(s))",
                            job.source_label,
                            findings.len()
                        ),
                    );
                    return CompletionReviewDisposition::RewriteOnly {
                        findings,
                        prior_body: reply_text.to_string(),
                        anchor_message_key: founder_reply_key.map(ToOwned::to_owned),
                        review_summary: outcome.summary.clone(),
                    };
                }
                if let Some(work_id) = resolve_review_rejection_target_self_work_id(root, job) {
                    push_event(
                        state,
                        format!(
                            "Founder review fail for {} will resume durable self-work {} instead of sending",
                            job.source_label, work_id
                        ),
                    );
                    CompletionReviewDisposition::RequeueSelfWork {
                        work_id,
                        summary: outcome.summary.clone(),
                    }
                } else if let Some(message_key) = founder_reply_key {
                    match enqueue_founder_communication_rework(root, job, message_key, &outcome) {
                        Ok(title) => {
                            push_event(
                                state,
                                format!(
                                    "Founder review fail for {} enqueued real communication rework via {}",
                                    job.source_label, title
                                ),
                            );
                            CompletionReviewDisposition::Hold {
                                summary: outcome.summary.clone(),
                            }
                        }
                        Err(err) => {
                            push_event(
                                state,
                                format!(
                                    "Founder review fail for {} could not enqueue communication rework: {}",
                                    job.source_label, err
                                ),
                            );
                            CompletionReviewDisposition::Hold {
                                summary: outcome.summary.clone(),
                            }
                        }
                    }
                } else {
                    CompletionReviewDisposition::Hold {
                        summary: outcome.summary.clone(),
                    }
                }
            }
            _ => CompletionReviewDisposition::Hold {
                summary: outcome.summary.clone(),
            },
        };
    }
    if let (Some(anchor_key), Some(action)) = (
        founder_outbound_anchor_key(job),
        proactive_founder_action.as_ref(),
    ) {
        return match outcome.verdict {
            review::ReviewVerdict::Pass => {
                if let Err(err) = channels::record_founder_outbound_review_approval(
                    root,
                    anchor_key,
                    action,
                    reply_text,
                    &outcome.summary,
                ) {
                    push_event(
                        state,
                        format!(
                            "Founder outbound review passed for {} but approval persistence failed: {}",
                            job.source_label, err
                        ),
                    );
                    CompletionReviewDisposition::Hold {
                        summary: err.to_string(),
                    }
                } else {
                    CompletionReviewDisposition::Approved
                }
            }
            review::ReviewVerdict::Fail | review::ReviewVerdict::Partial
                if actionable_rejection =>
            {
                if let Some(disposition) = no_cascade_review_block(root, job, &outcome) {
                    return disposition;
                }
                if matches!(routing_class, ReviewRoutingClass::Stale) {
                    return CompletionReviewDisposition::FeedbackRetry {
                        feedback_prompt: build_review_feedback_retry_prompt(
                            job, &outcome, reply_text,
                        ),
                        review_summary: outcome.summary.clone(),
                    };
                }
                if matches!(routing_class, ReviewRoutingClass::RewriteOnly) {
                    let findings = rewrite_findings_from(&outcome.categorized_findings);
                    push_event(
                        state,
                        format!(
                            "Founder outbound review for {} routed to lightweight rewrite-only path ({} finding(s))",
                            job.source_label,
                            findings.len()
                        ),
                    );
                    return CompletionReviewDisposition::RewriteOnly {
                        findings,
                        prior_body: reply_text.to_string(),
                        anchor_message_key: Some(anchor_key.to_string()),
                        review_summary: outcome.summary.clone(),
                    };
                }
                CompletionReviewDisposition::FeedbackRetry {
                    feedback_prompt: build_review_feedback_retry_prompt(job, &outcome, reply_text),
                    review_summary: outcome.summary.clone(),
                }
            }
            _ => CompletionReviewDisposition::Hold {
                summary: outcome.summary.clone(),
            },
        };
    }
    if actionable_rejection {
        if let Some(disposition) = no_cascade_review_block(root, job, &outcome) {
            return disposition;
        }
        if matches!(routing_class, ReviewRoutingClass::Stale) {
            match enqueue_review_stale_refresh(root, job, &outcome) {
                Ok(title) => push_event(state, format!("Review stale refresh enqueued: {title}")),
                Err(err) => push_event(
                    state,
                    format!(
                        "Review stale refresh enqueue failed for {}: {}",
                        job.source_label, err
                    ),
                ),
            }
            return CompletionReviewDisposition::Hold {
                summary: outcome.summary.clone(),
            };
        }
        return handle_actionable_completion_review_rejection(root, state, job, &outcome);
    }
    match outcome.verdict {
        review::ReviewVerdict::Pass => CompletionReviewDisposition::Approved,
        review::ReviewVerdict::Skipped => CompletionReviewDisposition::None,
        review::ReviewVerdict::Unavailable => {
            completion_review_unavailable_disposition(job, &outcome.summary)
        }
        review::ReviewVerdict::Fail | review::ReviewVerdict::Partial => {
            CompletionReviewDisposition::Hold {
                summary: outcome.summary.clone(),
            }
        }
    }
}

fn completion_review_unavailable_disposition(
    job: &QueuedPrompt,
    summary: &str,
) -> CompletionReviewDisposition {
    if is_terminal_bench_controller_artifact_job(job) {
        return CompletionReviewDisposition::Hold {
            summary: summary.to_string(),
        };
    }

    CompletionReviewDisposition::None
}

fn completion_review_should_skip_feedback_turn(job: &QueuedPrompt) -> bool {
    job.source_label == "review-feedback"
}

fn continuation_self_work_requested<'a>(
    job: &'a QueuedPrompt,
    reply_text: &str,
) -> Option<&'a str> {
    let work_id = job.ticket_self_work_id.as_deref()?;
    if is_founder_or_owner_email_job(job)
        || job.outbound_email.is_some()
        || job.source_label == REVIEW_REWRITE_SOURCE_LABEL
    {
        return None;
    }
    let lowered = reply_text.to_ascii_lowercase();
    let explicit_continue = contains_any(
        &lowered,
        &[
            "mach weiter",
            "weiter machen",
            "weiterarbeiten",
            "continue",
            "i can continue",
            "ich kann weiter",
            "next step",
            "next steps",
            "next slice",
            "nächster schritt",
            "naechster schritt",
        ],
    );
    if !explicit_continue {
        return None;
    }
    let has_open_work = contains_any(
        &lowered,
        &[
            "noch offen",
            "offene nächste",
            "offene naechste",
            "remaining",
            "pending",
            "not finished",
            "nicht fertig",
            "next smallest",
            "nächste konkrete",
            "naechste konkrete",
        ],
    );
    let claims_done = contains_any(
        &lowered,
        &[
            "fertig",
            "completed",
            "done",
            "abgeschlossen",
            "keine offenen",
            "no open",
        ],
    );
    if has_open_work || !claims_done {
        Some(work_id)
    } else {
        None
    }
}

/// Background-driven slices (timeout continuation, queue-pressure guard, cron,
/// and legacy watchdog items) are not directly owner-visible. The
/// owner_visible flag feeds the review-trigger heuristic, so we err on the
/// side of conservative review: foreground sources (TUI, queue, ticket
/// channels, email) are owner-visible.
fn derive_owner_visible_for_review(source_label: &str) -> bool {
    let lowered = source_label.to_ascii_lowercase();
    if lowered == QUEUE_GUARD_SOURCE_LABEL {
        return false;
    }
    !(lowered.contains("watchdog") || lowered.contains("timeout") || lowered.starts_with("cron"))
}

/// Synthesise the in-process `QueuedPrompt` that drives the lightweight
/// rewrite-only retry. The new prompt inherits outbound recipient/anchor
/// metadata from the parent job verbatim — no re-derivation, no leak of
/// internal vocab into the agent-facing instruction beyond what the
/// reviewer already surfaced.
fn synthesise_review_rewrite_prompt(
    parent: &QueuedPrompt,
    findings: &[RewriteFinding],
    prior_body: &str,
    anchor_message_key: Option<&str>,
    review_summary: &str,
) -> QueuedPrompt {
    let prompt = build_review_rewrite_prompt(prior_body, findings, anchor_message_key);
    QueuedPrompt {
        prompt,
        goal: format!("Body rewrite for {}", parent.source_label),
        preview: clip_text(review_summary, 160),
        source_label: REVIEW_REWRITE_SOURCE_LABEL.to_string(),
        suggested_skill: parent.suggested_skill.clone(),
        leased_message_keys: Vec::new(),
        leased_ticket_event_keys: Vec::new(),
        thread_key: parent.thread_key.clone(),
        workspace_root: parent.workspace_root.clone(),
        ticket_self_work_id: None,
        outbound_email: parent.outbound_email.clone(),
        outbound_anchor: parent.outbound_anchor.clone(),
    }
}

/// Project the `Rewrite`-class half of a reviewer's `categorized_findings`
/// list onto the dispatcher's `RewriteFinding` shape. Items tagged `Rework`
/// stay in the heavy-path payload elsewhere; this helper does not coerce
/// categories.
fn rewrite_findings_from(findings: &[review::CategorizedFinding]) -> Vec<RewriteFinding> {
    findings
        .iter()
        .filter(|f| matches!(f.category, review::FindingCategory::Rewrite))
        .map(|f| RewriteFinding {
            id: f.id.clone(),
            evidence: f.evidence.clone(),
            corrective_action: f.corrective_action.clone(),
        })
        .collect()
}

fn no_cascade_review_block(
    root: &Path,
    job: &QueuedPrompt,
    outcome: &review::ReviewOutcome,
) -> Option<CompletionReviewDisposition> {
    let work_id = job.ticket_self_work_id.as_deref()?;
    let target_work_id = resolve_review_rejection_target_self_work_id(root, job)
        .unwrap_or_else(|| work_id.to_string());
    let item = tickets::load_ticket_self_work_item(root, work_id)
        .ok()
        .flatten();
    let kind = item
        .as_ref()
        .map(|item| item.kind.as_str())
        .unwrap_or("unknown");
    let checkpoint_proof =
        match enforce_review_checkpoint_feedback_transition(root, &target_work_id, outcome) {
            Ok(proof_id) => proof_id,
            Err(err) => {
                return Some(CompletionReviewDisposition::Hold {
                    summary: format!(
                        "Review checkpoint rejected by core state machine for `{}`: {}",
                        target_work_id, err
                    ),
                });
            }
        };
    Some(CompletionReviewDisposition::RequeueSelfWork {
        work_id: target_work_id,
        summary: format!(
            "Review failed for durable self-work kind `{}`. Core checkpoint proof `{}` accepted. Feed this review back into the same main-agent work item; do not spawn review-owned rework: {}",
            kind,
            checkpoint_proof,
            outcome.summary
        ),
    })
}

fn expected_outcome_artifacts_for_job(job: &QueuedPrompt) -> Vec<ArtifactRef> {
    let mut refs = Vec::new();
    let workspace_terminal_state = if workspace_file_artifacts_require_fresh_write(job) {
        "fresh"
    } else {
        "present"
    };
    if let Some(action) = job.outbound_email.as_ref() {
        refs.push(ArtifactRef {
            kind: ArtifactKind::OutboundEmail,
            primary_key: outcome_thread_artifact_key(&action.thread_key),
            expected_terminal_state: "accepted".to_string(),
        });
    } else if is_founder_or_owner_email_job(job) {
        refs.push(ArtifactRef {
            kind: ArtifactKind::OutboundEmail,
            primary_key: job
                .thread_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(outcome_thread_artifact_key)
                .unwrap_or_else(|| "*".to_string()),
            expected_terminal_state: "accepted".to_string(),
        });
    } else if prompt_declares_reviewed_founder_send(&job.prompt) {
        refs.push(ArtifactRef {
            kind: ArtifactKind::OutboundEmail,
            primary_key: job
                .thread_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(outcome_thread_artifact_key)
                .unwrap_or_else(|| "*".to_string()),
            expected_terminal_state: "accepted".to_string(),
        });
    }
    for path in declared_workspace_file_artifacts_for_job(job) {
        if refs.iter().any(|existing| {
            existing.kind == ArtifactKind::WorkspaceFile && existing.primary_key == path
        }) {
            continue;
        }
        refs.push(ArtifactRef {
            kind: ArtifactKind::WorkspaceFile,
            primary_key: path,
            expected_terminal_state: workspace_terminal_state.to_string(),
        });
    }
    refs
}

fn delivered_outcome_artifacts_for_job(
    root: &Path,
    job: &QueuedPrompt,
    expected_artifact_refs: &[ArtifactRef],
) -> Result<Vec<ArtifactRef>> {
    if expected_artifact_refs.is_empty() {
        return Ok(Vec::new());
    }
    let conn = channels::open_channel_db(&crate::paths::core_db(&root))?;
    let fresh_cutoff = workspace_artifact_fresh_cutoff_for_job(root, job);
    let mut delivered = Vec::new();
    for expected in expected_artifact_refs {
        if expected.kind == ArtifactKind::WorkspaceFile {
            let path = Path::new(&expected.primary_key);
            if path.is_file()
                && (expected.expected_terminal_state != "fresh"
                    || workspace_file_is_fresh_enough(path, fresh_cutoff))
            {
                delivered.push(expected.clone());
            }
            continue;
        }
        if expected.kind != ArtifactKind::OutboundEmail {
            continue;
        }
        let message_key = if let Some(thread_key) = expected.primary_key.strip_prefix("thread:") {
            conn.query_row(
                r#"
                SELECT message_key
                FROM communication_messages
                WHERE channel = 'email'
                  AND direction = 'outbound'
                  AND thread_key = ?1
                  AND status = ?2
                ORDER BY observed_at DESC
                LIMIT 1
                "#,
                params![thread_key, expected.expected_terminal_state.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        } else if expected.primary_key == "*" {
            conn.query_row(
                r#"
                SELECT message_key
                FROM communication_messages
                WHERE channel = 'email'
                  AND direction = 'outbound'
                  AND status = ?1
                ORDER BY observed_at DESC
                LIMIT 1
                "#,
                params![expected.expected_terminal_state.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        } else {
            conn.query_row(
                r#"
                SELECT message_key
                FROM communication_messages
                WHERE message_key = ?1
                  AND status = ?2
                LIMIT 1
                "#,
                params![
                    expected.primary_key.as_str(),
                    expected.expected_terminal_state.as_str()
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        };
        if let Some(message_key) = message_key {
            delivered.push(ArtifactRef {
                kind: ArtifactKind::OutboundEmail,
                primary_key: message_key,
                expected_terminal_state: expected.expected_terminal_state.clone(),
            });
        }
    }
    Ok(delivered)
}

fn workspace_file_artifacts_require_fresh_write(job: &QueuedPrompt) -> bool {
    if declared_workspace_file_artifacts_for_job(job).is_empty() {
        return false;
    }
    if is_terminal_bench_controller_artifact_job(job) {
        return true;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}\n{}",
        job.prompt,
        job.goal,
        job.preview,
        job.thread_key.clone().unwrap_or_default(),
        job.workspace_root.clone().unwrap_or_default()
    )
    .to_ascii_lowercase();
    haystack.contains("checkpoint-only")
        || haystack.contains("checkpoint only")
        || haystack.contains("write a durable checkpoint")
        || haystack.contains("write a checkpoint")
        || haystack.contains("required output files to update now")
        || haystack.contains("required files to update now")
        || haystack.contains("preserve and update")
        || haystack.contains("update the required files")
        || haystack.contains("must update")
        || haystack.contains("update now")
}

fn workspace_artifact_fresh_cutoff_for_job(root: &Path, job: &QueuedPrompt) -> Option<SystemTime> {
    let mut cutoff = None;
    for message_key in &job.leased_message_keys {
        let Ok(Some(task)) = channels::load_queue_task(root, message_key) else {
            continue;
        };
        for value in [task.leased_at.as_deref(), Some(task.created_at.as_str())]
            .into_iter()
            .flatten()
        {
            if let Some(time) = parse_rfc3339_system_time(value) {
                if cutoff.map_or(true, |current| time > current) {
                    cutoff = Some(time);
                }
            }
        }
    }
    cutoff
}

fn parse_rfc3339_system_time(value: &str) -> Option<SystemTime> {
    let parsed = chrono::DateTime::parse_from_rfc3339(value).ok()?;
    let secs = parsed.timestamp();
    if secs < 0 {
        return None;
    }
    Some(
        UNIX_EPOCH
            + Duration::from_secs(secs as u64)
            + Duration::from_nanos(parsed.timestamp_subsec_nanos() as u64),
    )
}

fn workspace_file_is_fresh_enough(path: &Path, cutoff: Option<SystemTime>) -> bool {
    let Some(cutoff) = cutoff else {
        return true;
    };
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    modified.duration_since(cutoff).is_ok()
}

fn declared_workspace_file_artifacts_for_job(job: &QueuedPrompt) -> Vec<String> {
    let prompt = job.prompt.as_str();
    if !prompt_declares_workspace_file_artifact(prompt) {
        return Vec::new();
    }
    let explicit_only = extract_only_required_durable_file_paths(prompt);
    if !explicit_only.is_empty() {
        return explicit_only;
    }

    let mut refs = Vec::new();
    let mut base_dirs = extract_declared_artifact_base_dirs(prompt);
    if base_dirs.is_empty() {
        if let Some(root) = job
            .workspace_root
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            base_dirs.push(root.to_string());
        }
    }

    for path in extract_absolute_workspace_file_paths(prompt) {
        push_unique_string(&mut refs, path);
    }
    for name in extract_relative_artifact_file_names(prompt) {
        for base in &base_dirs {
            push_unique_string(
                &mut refs,
                Path::new(base).join(&name).to_string_lossy().into_owned(),
            );
        }
    }
    refs
}

fn artifact_first_execution_prompt(job: &QueuedPrompt) -> String {
    let file_refs = declared_workspace_file_artifacts_for_job(job);
    if file_refs.is_empty() {
        return job.prompt.clone();
    }

    if is_terminal_bench_controller_artifact_job(job)
        && terminal_bench_first_turn_preflight_applies(job, &file_refs)
    {
        return terminal_bench_controller_artifact_preflight_prompt(job, &file_refs);
    }

    let mut prompt = String::new();
    prompt.push_str("HARNESS ARTIFACT CONTRACT\n");
    prompt.push_str("This task declares durable file artifacts. The harness will not accept a final answer, plan, or interim text as completion unless these files exist on disk.\n\n");
    if let Some(workspace_root) = job
        .workspace_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("Workspace root:\n");
        prompt.push_str(workspace_root);
        prompt.push_str("\n\n");
    }
    prompt.push_str("Required files:\n");
    for path in &file_refs {
        prompt.push_str("- ");
        prompt.push_str(path);
        prompt.push('\n');
    }
    prompt.push_str("\nExecution order:\n");
    prompt.push_str("1. Before open-ended research or exploratory loops, create or update the required files with the best current status.\n");
    prompt.push_str("2. If final content depends on later work, write a provisional, truthful status plus the next action, then keep updating the file as work progresses.\n");
    prompt.push_str("3. Each required path must be a regular file. A directory at that path is invalid; move or remove the directory and create the file.\n");
    prompt.push_str("4. Use absolute paths from the Required files list, or explicitly `cd` into the Workspace root before relative writes. A file written under the install directory or any other cwd does not satisfy this task.\n");
    prompt.push_str("5. Before claiming completion, run shell checks equivalent to `test -f` for every required path.\n");
    prompt.push_str("6. If a required file cannot be created, write the blocker into the files that can be created and do not claim completion.\n\n");
    prompt.push_str("ORIGINAL TASK\n");
    prompt.push_str(&job.prompt);
    prompt
}

fn terminal_bench_controller_artifact_preflight_prompt(
    job: &QueuedPrompt,
    file_refs: &[String],
) -> String {
    let run_dir = terminal_bench_run_dir_from_artifact_paths(file_refs);
    let mut prompt = String::new();
    prompt.push_str("HARNESS TERMINAL-BENCH PREFLIGHT\n");
    prompt.push_str("The current Terminal-Bench controller task is paused at an artifact gate. The worker must perform this preflight itself. The harness and review system will not create files, create tickets, run commands, patch artifacts, or mark anything complete for the worker.\n\n");
    if let Some(run_dir) = run_dir.as_deref() {
        prompt.push_str("CURRENT TERMINAL-BENCH RUN SCOPE\n");
        prompt.push_str("Use exactly this RUN_DIR for this queue item:\n");
        prompt.push_str(run_dir);
        prompt.push_str("\n\n");
        prompt.push_str("Do not read, copy, or continue controller-prompt.md, controller.json, ticket-map.jsonl, preparation-tickets.jsonl, run-queue.jsonl, results.jsonl, knowledge.md, logbook.md, or blogpost-notes.md from any other Terminal-Bench run directory. Other directories under /home/metricspace/CTOX/runtime/terminal-bench-2/runs are stale context for this item. If a shell command or file path points at a different run id, that is a wrong-run error; stop that command sequence and write the blocker into the required files in the current RUN_DIR.\n\n");
    }
    prompt.push_str("FIRST TOOL CALL CONTRACT\n");
    prompt.push_str("Your next assistant turn must use exactly one shell/terminal tool call before any prose conclusion. That shell script must, in this order:\n");
    if let Some(run_dir) = run_dir.as_deref() {
        prompt.push_str("1. Run `mkdir -p ");
        prompt.push_str(run_dir);
        prompt.push_str("/tasks`.\n");
    } else {
        prompt.push_str("1. Create the directory that contains the required files and its tasks subdirectory.\n");
    }
    prompt.push_str("2. Create or update every required file listed below as a regular file with truthful current status.\n");
    prompt.push_str("3. If CTOX queue syntax is needed, run `ctox queue --help` or `ctox queue add --help` only after those files exist, and append the useful output or blocker to logbook.md.\n");
    if terminal_bench_controller_requires_runtime_refs(job) {
        prompt.push_str("4. Create durable CTOX queue/ticket work for the preparation phase and record the real message keys in ticket-map.jsonl and preparation-tickets.jsonl. Use the worker's CLI; do not invent synthetic keys.\n");
        prompt.push_str("5. Verify each queued item with `ctox queue show` when the CLI supports it, or record the exact CLI blocker in logbook.md and controller.json.\n");
        prompt.push_str("6. Run `test -f` checks for every required file and append the result to logbook.md.\n\n");
        prompt.push_str("Required preparation queue items:\n");
        prompt.push_str("- Inventory Terminal-Bench 2 tasks and create one durable benchmark ticket per task without opening solutions.\n");
        prompt.push_str("- Research public Terminal-Bench 2 reference results and safe task ordering without solution leakage.\n");
        prompt.push_str("- Verify the active harness runtime/model/provider, response adapter, and 131072 token context setting. If the runtime is local, record IPC/native/GPU facts; if it is remote/API-backed, record the provider/API path.\n");
        prompt.push_str("- Run an initial small set of likely-solvable Terminal-Bench 2 tasks under Harbor/Terminal-Bench tooling.\n");
        prompt.push_str(
            "- Update knowledge.md, results.jsonl, and logbook.md after each benchmark attempt.\n",
        );
        prompt.push_str("- Return later to failed or skipped tasks with accumulated learnings, without treating intermediate state as final success.\n\n");
    } else {
        prompt.push_str("4. Run `test -f` checks for every required file and append the result to logbook.md or the closest required log file.\n\n");
    }
    prompt.push_str("Forbidden before the above succeeds:\n");
    prompt.push_str("- `find`, `grep`, `rg`, `ls`, `cat`, `which`, `pip`, `python`, `docker`, `harbor`, or install-tree inspection unless the same shell script has already created every required file in the current RUN_DIR.\n");
    prompt.push_str("- Web research, benchmark execution, model evaluation, broad codebase discovery, or reading old Terminal-Bench run directories.\n");
    prompt.push_str("- A final answer that says work will be done later without the files and real queue refs existing on disk.\n\n");
    prompt.push_str("Required files:\n");
    for path in file_refs {
        prompt.push_str("- ");
        prompt.push_str(path);
        prompt.push('\n');
    }
    prompt.push_str("\nCompletion condition for this preflight turn:\n");
    prompt.push_str("End only after the shell has created the required files, recorded real queue refs or a precise CLI blocker, and verified every required path with `test -f`. If anything fails, write the failure into the required files and continue only with the next concrete repair step.\n\n");
    prompt.push_str("Original controller task is intentionally withheld until this artifact preflight exists. Use the preparation queue items above to carry the benchmark forward.");
    prompt
}

fn terminal_bench_run_dir_from_artifact_paths(paths: &[String]) -> Option<String> {
    for path in paths {
        let marker = "/terminal-bench-2/runs/";
        let Some(marker_start) = path.find(marker) else {
            continue;
        };
        let run_id_start = marker_start + marker.len();
        let rest = &path[run_id_start..];
        let Some(run_id_len) = rest.find('/') else {
            continue;
        };
        if run_id_len == 0 {
            continue;
        }
        return Some(path[..run_id_start + run_id_len].to_string());
    }
    None
}

fn is_terminal_bench_controller_artifact_job(job: &QueuedPrompt) -> bool {
    if declared_workspace_file_artifacts_for_job(job).is_empty() {
        return false;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}\n{}",
        job.prompt,
        job.goal,
        job.preview,
        job.thread_key.clone().unwrap_or_default(),
        job.workspace_root.clone().unwrap_or_default()
    )
    .to_ascii_lowercase();
    let terminal_bench_scope = haystack.contains("terminal-bench")
        || haystack.contains("terminal bench")
        || haystack.contains("tbench");
    let controller_shape = haystack.contains("controller")
        || haystack.contains("harbor")
        || haystack.contains("ticket-map.jsonl")
        || haystack.contains("results.json")
        || haystack.contains("run-log.md");
    terminal_bench_scope && controller_shape
}

fn terminal_bench_controller_requires_runtime_refs(job: &QueuedPrompt) -> bool {
    if !is_terminal_bench_controller_artifact_job(job) {
        return false;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}\n{}",
        job.prompt,
        job.goal,
        job.preview,
        job.thread_key.clone().unwrap_or_default(),
        job.workspace_root.clone().unwrap_or_default()
    )
    .to_ascii_lowercase();
    haystack.contains("create durable ctox queue/ticket work")
        || haystack.contains("preparation queue/tickets")
        || haystack.contains("preparation-tickets.jsonl")
        || haystack.contains("one benchmark ticket")
        || haystack.contains("message keys")
}

fn terminal_bench_preflight_spec_for_job(job: &QueuedPrompt) -> Option<TerminalBenchPreflightSpec> {
    if !is_terminal_bench_controller_artifact_job(job) {
        return None;
    }
    let file_refs = declared_workspace_file_artifacts_for_job(job);
    if !terminal_bench_first_turn_preflight_applies(job, &file_refs) {
        return None;
    }
    let run_dir = terminal_bench_run_dir_from_artifact_paths(&file_refs)?;
    Some(TerminalBenchPreflightSpec {
        run_dir,
        required_files: file_refs,
        requires_runtime_refs: terminal_bench_controller_requires_runtime_refs(job),
    })
}

fn terminal_bench_first_turn_preflight_applies(job: &QueuedPrompt, file_refs: &[String]) -> bool {
    if file_refs.is_empty() || !is_terminal_bench_controller_artifact_job(job) {
        return false;
    }
    if job.source_label == "review-feedback" || job.ticket_self_work_id.is_some() {
        return false;
    }
    let lower_prompt = job.prompt.to_ascii_lowercase();
    let explicit_first_turn_contract = [
        "only required durable files for this controller turn",
        "first action must be one shell",
        "critical first-turn contract",
        "harness terminal-bench preflight",
        "harness terminal-bench preflight retry",
    ]
    .iter()
    .any(|needle| lower_prompt.contains(needle));
    if !explicit_first_turn_contract {
        return false;
    }
    let requires_runtime_refs = terminal_bench_controller_requires_runtime_refs(job);
    !terminal_bench_preflight_already_bootstrapped(file_refs, requires_runtime_refs)
}

fn terminal_bench_preflight_already_bootstrapped(
    file_refs: &[String],
    requires_runtime_refs: bool,
) -> bool {
    let Some(run_dir) = terminal_bench_run_dir_from_artifact_paths(file_refs) else {
        return false;
    };
    let all_required_files_exist = file_refs.iter().all(|path| Path::new(path).is_file());
    if !all_required_files_exist || !Path::new(&run_dir).join("tasks").is_dir() {
        return false;
    }
    if !requires_runtime_refs {
        return true;
    }
    terminal_bench_file_refs_contain_real_queue_refs(file_refs)
}

fn terminal_bench_file_refs_contain_real_queue_refs(file_refs: &[String]) -> bool {
    file_refs.iter().any(|path| {
        let Ok(content) = std::fs::read_to_string(path) else {
            return false;
        };
        content.contains("queue:system::")
    })
}

fn validate_terminal_bench_controller_runtime_refs(
    root: &Path,
    job: &QueuedPrompt,
    expected_artifact_refs: &[ArtifactRef],
) -> Result<()> {
    if !terminal_bench_controller_requires_runtime_refs(job) {
        return Ok(());
    }
    let paths = terminal_bench_controller_runtime_ref_paths(expected_artifact_refs);
    if paths.is_empty() {
        return Ok(());
    }

    let mut valid_refs = Vec::new();
    let mut synthetic_refs = Vec::new();
    for path in &paths {
        if !path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        collect_valid_terminal_bench_runtime_refs(
            root,
            job,
            &text,
            &mut valid_refs,
            &mut synthetic_refs,
        )?;
    }

    if !valid_refs.is_empty() {
        return Ok(());
    }
    if terminal_bench_controller_has_explicit_blocker(expected_artifact_refs) {
        return Ok(());
    }

    anyhow::bail!(
        "Terminal-Bench controller outcome is missing real CTOX runtime ticket/queue refs. \
ticket-map.jsonl, preparation-tickets.jsonl, and run-queue.jsonl must reference existing \
CTOX queue message keys such as queue:system::<id> or existing ticket self-work IDs. \
Synthetic refs like {} do not count. Create real CTOX queue tasks via `ctox queue add` \
or persist an explicit blocker with the exact next command.",
        if synthetic_refs.is_empty() {
            "msg-prep-runtime-001".to_string()
        } else {
            synthetic_refs
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
}

fn terminal_bench_controller_runtime_ref_paths(
    expected_artifact_refs: &[ArtifactRef],
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for artifact in expected_artifact_refs {
        if artifact.kind != ArtifactKind::WorkspaceFile {
            continue;
        }
        let path = Path::new(&artifact.primary_key);
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if matches!(
            name,
            "ticket-map.jsonl" | "preparation-tickets.jsonl" | "run-queue.jsonl"
        ) {
            paths.push(path.to_path_buf());
        }
    }
    paths
}

fn collect_valid_terminal_bench_runtime_refs(
    root: &Path,
    job: &QueuedPrompt,
    text: &str,
    valid_refs: &mut Vec<String>,
    synthetic_refs: &mut Vec<String>,
) -> Result<()> {
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            collect_valid_terminal_bench_runtime_refs_from_value(
                root,
                job,
                &value,
                valid_refs,
                synthetic_refs,
            )?;
        } else {
            collect_valid_terminal_bench_runtime_refs_from_text(
                root,
                job,
                line,
                valid_refs,
                synthetic_refs,
            )?;
        }
    }
    Ok(())
}

fn collect_valid_terminal_bench_runtime_refs_from_value(
    root: &Path,
    job: &QueuedPrompt,
    value: &Value,
    valid_refs: &mut Vec<String>,
    synthetic_refs: &mut Vec<String>,
) -> Result<()> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if let Some(value) = value.as_str() {
                    validate_terminal_bench_runtime_ref_value(
                        root,
                        job,
                        key,
                        value,
                        valid_refs,
                        synthetic_refs,
                    )?;
                } else {
                    collect_valid_terminal_bench_runtime_refs_from_value(
                        root,
                        job,
                        value,
                        valid_refs,
                        synthetic_refs,
                    )?;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_valid_terminal_bench_runtime_refs_from_value(
                    root,
                    job,
                    item,
                    valid_refs,
                    synthetic_refs,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_valid_terminal_bench_runtime_refs_from_text(
    root: &Path,
    job: &QueuedPrompt,
    text: &str,
    valid_refs: &mut Vec<String>,
    synthetic_refs: &mut Vec<String>,
) -> Result<()> {
    for token in text
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ';' | '[' | ']'))
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        validate_terminal_bench_runtime_ref_value(
            root,
            job,
            "runtime_ref",
            token,
            valid_refs,
            synthetic_refs,
        )?;
    }
    Ok(())
}

fn validate_terminal_bench_runtime_ref_value(
    root: &Path,
    job: &QueuedPrompt,
    key: &str,
    value: &str,
    valid_refs: &mut Vec<String>,
    synthetic_refs: &mut Vec<String>,
) -> Result<()> {
    let key = key.to_ascii_lowercase();
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    if value.starts_with("msg-") || value.starts_with("queue-id-") {
        push_unique_string(synthetic_refs, value.to_string());
        return Ok(());
    }
    let queue_key_field =
        key.contains("message_key") || key.contains("queue_key") || key == "runtime_ref";
    if queue_key_field && value.starts_with("queue:") {
        if job
            .leased_message_keys
            .iter()
            .any(|leased| leased.as_str() == value)
        {
            return Ok(());
        }
        if channels::load_queue_task(root, value)?.is_some() {
            push_unique_string(valid_refs, value.to_string());
        }
        return Ok(());
    }
    let work_id_field =
        key.contains("work_id") || key.contains("self_work_id") || key.contains("ticket_work");
    if work_id_field && tickets::load_ticket_self_work_item(root, value)?.is_some() {
        push_unique_string(valid_refs, value.to_string());
    }
    Ok(())
}

fn terminal_bench_controller_has_explicit_blocker(expected_artifact_refs: &[ArtifactRef]) -> bool {
    let mut controller_has_blocker = false;
    let mut next_action_recorded = false;
    for artifact in expected_artifact_refs {
        if artifact.kind != ArtifactKind::WorkspaceFile {
            continue;
        }
        let path = Path::new(&artifact.primary_key);
        if !path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let lowered = text.to_ascii_lowercase();
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name == "controller.json")
            && (lowered.contains("\"blocker\"")
                || lowered.contains("\"status\":\"blocked\"")
                || lowered.contains("\"phase\":\"blocked\""))
        {
            controller_has_blocker = true;
        }
        if lowered.contains("next_action")
            || lowered.contains("next command")
            || lowered.contains("next_command")
        {
            next_action_recorded = true;
        }
    }
    controller_has_blocker && next_action_recorded
}

fn maybe_terminal_bench_controller_runtime_ref_feedback(
    root: &Path,
    job: &QueuedPrompt,
) -> Result<Option<String>> {
    if !terminal_bench_controller_requires_runtime_refs(job) {
        return Ok(None);
    }
    let expected = expected_outcome_artifacts_for_job(job);
    let runtime_ref_paths = terminal_bench_controller_runtime_ref_paths(&expected);
    if runtime_ref_paths.is_empty() {
        return Ok(None);
    }
    let current_run_dir = terminal_bench_run_dir_from_artifact_paths(
        &expected
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::WorkspaceFile)
            .map(|artifact| artifact.primary_key.clone())
            .collect::<Vec<_>>(),
    );
    let mut valid_refs = Vec::new();
    let mut synthetic_refs = Vec::new();
    for path in &runtime_ref_paths {
        if path.is_file() {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            collect_valid_terminal_bench_runtime_refs(
                root,
                job,
                &text,
                &mut valid_refs,
                &mut synthetic_refs,
            )?;
        }
    }
    if !valid_refs.is_empty() {
        return Ok(Some(terminal_bench_runtime_ref_feedback_note(
            &valid_refs,
            current_run_dir.as_deref(),
            "The harness found existing real preparation queue refs.",
        )));
    }

    let parent_key = parent_queue_key_for_feedback(job);
    let example = parent_key
        .as_deref()
        .map(|key| {
            format!(
                "Concrete valid command shape for each preparation item:\n\
ctox queue add --title \"<short preparation title>\" --prompt \"<full worker instruction>\" --thread-key \"terminal-bench-2/prep/<slug>\" --workspace-root \"{}\" --skill benchmark-controller --priority high --parent-message-key {key}\n\
Do not use `--description`; this CLI requires `--prompt <text>`.",
                current_run_dir.as_deref().unwrap_or("<current RUN_DIR>")
            )
        })
        .unwrap_or_else(|| {
            format!(
                "Concrete valid command shape for each preparation item:\n\
ctox queue add --title \"<short preparation title>\" --prompt \"<full worker instruction>\" --thread-key \"terminal-bench-2/prep/<slug>\" --workspace-root \"{}\" --skill benchmark-controller --priority high\n\
Do not use `--description`; this CLI requires `--prompt <text>`.",
                current_run_dir.as_deref().unwrap_or("<current RUN_DIR>")
            )
        });
    Ok(Some(format!(
        "HARNESS FEEDBACK\n\
The Terminal-Bench controller requires real CTOX runtime queue/ticket refs, but none are currently recorded in ticket-map.jsonl, preparation-tickets.jsonl, or run-queue.jsonl.\n\
Current RUN_DIR for this queue item: {}\n\
Write only to files in this RUN_DIR. Do not inspect or reuse controller-prompt.md or durable files from older Terminal-Bench run directories.\n\
Do not invent identifiers. Values like msg-prep-runtime-001, q1, ticket-1, or TODO are invalid.\n\
If you are unsure about CTOX CLI syntax, inspect it yourself with `ctox help`, `ctox queue --help`, and `ctox queue add --help` before creating any tickets. Do not guess.\n\
Your next shell action must create the preparation work yourself with `ctox queue add --title ... --prompt ...`; `--description` is not a valid `ctox queue add` flag. Capture the real `queue:system::*` message_key values from stdout, verify each with `ctox queue show --message-key <key>`, and persist those exact keys in ticket-map.jsonl, preparation-tickets.jsonl, and run-queue.jsonl before any benchmark work.\n\
{example}\n\
If any `ctox queue add` command fails, persist a blocker in controller.json, logbook.md, and run-queue.jsonl with the exact failing command and stderr. Do not claim completion."
        ,
        current_run_dir.as_deref().unwrap_or("<declared artifact paths>")
    )))
}

fn parent_queue_key_for_feedback(job: &QueuedPrompt) -> Option<String> {
    job.leased_message_keys
        .iter()
        .find(|key| key.starts_with("queue:"))
        .cloned()
}

fn terminal_bench_runtime_ref_feedback_note(
    refs: &[String],
    current_run_dir: Option<&str>,
    action: &str,
) -> String {
    format!(
        "HARNESS FEEDBACK\n\
{action}\n\
These refs are real CTOX runtime objects already persisted in the run artifacts:\n\
{}\n\n\
Current RUN_DIR for this queue item: {}\n\
Do not create duplicate preparation queue tasks. The harness is only pointing out the state; it will not perform the work for you. Read controller.json, ticket-map.jsonl, preparation-tickets.jsonl, run-queue.jsonl, knowledge.md, and logbook.md from this RUN_DIR first. If you need CLI syntax, inspect `ctox help` and the relevant subcommand `--help` yourself. Do not read or continue stale Terminal-Bench run directories. Continue by verifying the queued preparation work and updating the durable files with any new facts.",
        refs.iter()
            .map(|value| format!("- {value}"))
            .collect::<Vec<_>>()
            .join("\n")
            ,
        current_run_dir.unwrap_or("<declared artifact paths>")
    )
}

fn terminal_bench_controller_hold_feedback_prompt(
    job: &QueuedPrompt,
    review_summary: &str,
) -> String {
    let file_refs = declared_workspace_file_artifacts_for_job(job);
    let run_dir = terminal_bench_run_dir_from_artifact_paths(&file_refs);
    let basenames = file_refs
        .iter()
        .filter_map(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    let artifact_names = if basenames.is_empty() {
        "the declared durable files".to_string()
    } else {
        basenames.join(", ")
    };
    let results_name = basenames
        .iter()
        .find(|name| name.starts_with("results."))
        .cloned()
        .unwrap_or_else(|| "results.jsonl".to_string());
    let log_name = basenames
        .iter()
        .find(|name| *name == "logbook.md" || *name == "run-log.md")
        .cloned()
        .unwrap_or_else(|| "logbook.md".to_string());
    let mut prompt = String::new();
    prompt.push_str("HARNESS FEEDBACK\n");
    prompt.push_str(
        "The previous Terminal-Bench controller turn is not accepted as benchmark completion. ",
    );
    prompt.push_str(&format!("The durable files may have been initialized, but the benchmark controller must continue until Terminal-Bench work has real task statuses in {results_name} or a concrete persisted next action after exhausting the current time budget.\n\n"));
    prompt.push_str("Review status:\n");
    prompt.push_str("- ");
    prompt.push_str(&clip_text(review_summary, 500));
    prompt.push_str("\n\n");
    if let Some(run_dir) = run_dir.as_deref() {
        prompt.push_str("Current RUN_DIR:\n");
        prompt.push_str(run_dir);
        prompt.push_str("\n\n");
    }
    prompt.push_str("Required continuation behavior:\n");
    prompt.push_str("- The harness is only giving feedback. It will not perform the benchmark work, create tickets, patch artifacts, or mark tasks complete for you. You must do the work yourself with shell tools and CTOX CLI commands.\n");
    prompt.push_str("- If you are unsure about CTOX CLI syntax or available commands, inspect it yourself with `ctox help`, `ctox queue --help`, `ctox queue add --help`, and the relevant subcommand `--help` before acting.\n");
    prompt.push_str(&format!("- Read only the current RUN_DIR durable files ({artifact_names}) far enough to preserve existing progress. Do not recreate them from scratch and do not read stale Terminal-Bench run directories.\n"));
    prompt.push_str("- Your next shell action after this feedback must update the current RUN_DIR durable files, not just inspect them. A valid first action may read the current files and write the checkpoint in the same shell script.\n");
    prompt.push_str(&format!("- Before any open-ended discovery, runner probing, benchmark execution, or web research, write a checkpoint into the declared durable files, especially controller.json, {log_name}, knowledge.md, and {results_name}. The checkpoint must record the current phase, verified facts, blockers, and exact next action.\n"));
    prompt.push_str("- If a tool call finds new runtime, task, runner, leaderboard, blocker, or result information, the next tool call must persist that information into the current RUN_DIR durable files before continuing exploration.\n");
    prompt.push_str(&format!("- Treat controller.phase=preparation and {results_name} with zero real task statuses as an unfinished state, not as completion.\n"));
    prompt.push_str("- Verify the runtime facts before benchmark execution: CTOX release is current, active harness model/provider match this run, response adapter is correct, and effective context is 131072 tokens. If inference is local, record native runtime/IPC/GPU evidence; if inference is API-backed, record the provider/API evidence instead of inventing local-only facts.\n");
    prompt.push_str("- Verify Harbor and the Terminal-Bench 2 task source, then write one ticket per discovered benchmark task into ticket-map.jsonl.\n");
    prompt.push_str("- Research public Terminal-Bench references and leaderboards only for task selection and comparison context; do not read benchmark solutions.\n");
    prompt.push_str("- Start with tasks known to be solvable by other harnesses/models, update knowledge.md after each attempt, skip blocked tasks temporarily, and return later with accumulated learnings.\n");
    prompt.push_str(&format!("- After every benchmark action, update {log_name} and {results_name} truthfully. Each task must end as passed, failed, blocked, skipped, or pending with evidence.\n"));
    prompt.push_str(&format!("- Only finish after all discovered Terminal-Bench tasks have terminal statuses, or after the current time budget is exhausted with a single explicit persisted next_action in controller.json and matching {log_name}.\n\n"));
    if !file_refs.is_empty() {
        prompt.push_str("Durable files to preserve and update:\n");
        for path in &file_refs {
            prompt.push_str("- ");
            prompt.push_str(path);
            prompt.push('\n');
        }
        prompt.push('\n');
    }
    prompt.push_str("Continue now from the persisted state. Use shell tools and direct file checks for evidence. Do not wait for the harness to do any work for you. Do not answer with prose only.");
    prompt
}

fn extract_only_required_durable_file_paths(prompt: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut in_section = false;
    for line in prompt.lines() {
        let lowered = line.to_ascii_lowercase();
        if lowered.contains("only required durable files")
            || lowered.contains("only required durable file")
            || lowered.contains("durable artifact contract")
            || lowered.contains("required artifacts")
            || lowered.contains("required durable files")
            || lowered.contains("required files:")
            || lowered.contains("create these files")
            || lowered.contains("create these five files")
            || lowered.contains("exact files")
            || lowered.contains("these exact files")
        {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        if line.trim().is_empty() {
            if !refs.is_empty() {
                break;
            }
            continue;
        }
        if !refs.is_empty()
            && (lowered.ends_with(':')
                || lowered.starts_with("initial ")
                || lowered.starts_with("completion ")
                || lowered.starts_with("success ")
                || lowered.starts_with("start "))
        {
            break;
        }
        let paths = extract_absolute_workspace_file_paths(line);
        if paths.is_empty() && !refs.is_empty() && !line.trim_start().starts_with('-') {
            break;
        }
        for path in paths {
            push_unique_string(&mut refs, path);
        }
    }
    refs
}

fn workspace_file_artifact_diagnostic(path: &str) -> &'static str {
    let path = Path::new(path);
    if path.is_file() {
        "ok: regular file exists"
    } else if path.is_dir() {
        "invalid: exists as a directory; required path must be a regular file"
    } else if path.exists() {
        "invalid: exists but is not a regular file"
    } else {
        "missing: regular file not found"
    }
}

fn workspace_file_artifact_diagnostic_for_expected(
    root: &Path,
    job: &QueuedPrompt,
    artifact: &ArtifactRef,
) -> String {
    let path = Path::new(&artifact.primary_key);
    if artifact.expected_terminal_state == "fresh"
        && path.is_file()
        && !workspace_file_is_fresh_enough(path, workspace_artifact_fresh_cutoff_for_job(root, job))
    {
        return "stale: regular file exists, but it was not updated after the current queue lease; this turn must write or touch the file with truthful current state".to_string();
    }
    workspace_file_artifact_diagnostic(&artifact.primary_key).to_string()
}

fn prompt_declares_workspace_file_artifact(prompt: &str) -> bool {
    let lowered = prompt.to_ascii_lowercase();
    let artifact_words = [
        "artefakt",
        "artifact",
        "datei",
        "file",
        "schreiben",
        "speichern",
        "initialisieren",
        "initialise",
        "initialize",
        "write",
        "create",
        "append",
    ];
    artifact_words.iter().any(|word| lowered.contains(word))
}

fn extract_declared_artifact_base_dirs(prompt: &str) -> Vec<String> {
    let mut dirs = Vec::new();
    for line in prompt.lines() {
        let lowered = line.to_ascii_lowercase();
        if !(lowered.contains("run_dir")
            || lowered.contains("workspace")
            || lowered.contains("arbeitsordner"))
        {
            continue;
        }
        for path in extract_absolute_paths_from_text(line) {
            push_unique_string(&mut dirs, path);
        }
    }
    dirs
}

fn extract_absolute_workspace_file_paths(prompt: &str) -> Vec<String> {
    extract_absolute_paths_from_text(prompt)
        .into_iter()
        .filter(|path| artifact_file_name(path))
        .collect()
}

fn extract_absolute_paths_from_text(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut current = String::new();
    let mut previous = None;
    for ch in text.chars().chain(std::iter::once(' ')) {
        if current.is_empty() {
            if ch == '/' && is_absolute_path_start_boundary(previous) {
                current.push(ch);
            }
            previous = Some(ch);
            continue;
        }
        if is_path_char(ch) {
            current.push(ch);
        } else {
            let trimmed = current
                .trim_matches(|c: char| {
                    matches!(
                        c,
                        '"' | '\'' | '`' | ')' | ']' | '}' | '.' | ',' | ';' | ':'
                    )
                })
                .to_string();
            if !trimmed.is_empty() {
                push_unique_string(&mut paths, trimmed);
            }
            current.clear();
        }
        previous = Some(ch);
    }
    paths
}

fn is_absolute_path_start_boundary(previous: Option<char>) -> bool {
    previous.is_none_or(|ch| {
        ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '(' | '[' | '{' | '<' | '=')
    })
}

fn extract_relative_artifact_file_names(prompt: &str) -> Vec<String> {
    let mut names = Vec::new();
    for token in prompt.split(|ch: char| !is_relative_artifact_token_char(ch)) {
        let trimmed =
            token.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | ',' | ';' | ':' | '.'));
        if trimmed.is_empty()
            || trimmed.starts_with('/')
            || trimmed.contains("://")
            || trimmed.contains('$')
        {
            continue;
        }
        if artifact_file_name(trimmed) {
            push_unique_string(&mut names, trimmed.to_string());
        }
    }
    names
}

fn artifact_file_name(path: &str) -> bool {
    let Some(name) = Path::new(path).file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lowered = name.to_ascii_lowercase();
    [
        ".md", ".json", ".jsonl", ".txt", ".csv", ".tsv", ".log", ".yaml", ".yml", ".toml",
        ".sqlite", ".sqlite3",
    ]
    .iter()
    .any(|suffix| lowered.ends_with(suffix))
}

fn is_path_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-' | '.' | ':' | '=')
}

fn is_relative_artifact_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/')
}

fn push_unique_string(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn outcome_thread_artifact_key(thread_key: &str) -> String {
    format!("thread:{}", thread_key.trim())
}

fn prompt_declares_reviewed_founder_send(prompt: &str) -> bool {
    let lowered = prompt.to_ascii_lowercase();
    lowered.contains("reviewed-founder-send")
        || lowered.contains("reviewed founder send")
        || lowered.contains("founder-outbound")
        || lowered.contains("owner/founder/admin-targeted outbound email")
}

fn enforce_job_outcome_witness(
    root: &Path,
    job: &QueuedPrompt,
    expected_artifact_refs: Vec<ArtifactRef>,
    delivered_artifact_refs: Vec<ArtifactRef>,
) -> Result<Option<String>> {
    if expected_artifact_refs.is_empty() {
        return Ok(None);
    }
    validate_terminal_bench_controller_runtime_refs(root, job, &expected_artifact_refs)?;

    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let entity_id = job_outcome_entity_id(job);
    let (entity_type, from_state, to_state, event) = if job.ticket_self_work_id.is_some() {
        (
            CoreEntityType::WorkItem,
            CoreState::Verified,
            CoreState::Closed,
            CoreEvent::Close,
        )
    } else {
        (
            CoreEntityType::QueueItem,
            CoreState::Running,
            CoreState::Completed,
            CoreEvent::Complete,
        )
    };

    let proof = enforce_core_transition(
        &conn,
        &CoreTransitionRequest {
            entity_type,
            entity_id,
            lane: RuntimeLane::P0FounderCommunication,
            from_state,
            to_state,
            event,
            actor: "ctox-outcome-witness".to_string(),
            evidence: CoreEvidenceRefs {
                verification_id: Some(format!(
                    "outcome-witness:{}",
                    channels::stable_digest(&job.source_label)
                )),
                expected_artifact_refs,
                delivered_artifact_refs,
                ..CoreEvidenceRefs::default()
            },
            metadata: BTreeMap::from([
                ("outcome_witness".to_string(), "true".to_string()),
                ("source_label".to_string(), job.source_label.clone()),
            ]),
        },
    )?;
    Ok(Some(proof.proof_id))
}

fn outcome_witness_recovery_message(
    root: &Path,
    job: &QueuedPrompt,
    approved_body: &str,
    err: &str,
) -> String {
    let expected_file_artifacts = expected_outcome_artifacts_for_job(job)
        .into_iter()
        .filter(|artifact| artifact.kind == ArtifactKind::WorkspaceFile)
        .collect::<Vec<_>>();
    let file_refs = expected_file_artifacts
        .iter()
        .map(|artifact| artifact.primary_key.clone())
        .collect::<Vec<_>>();
    let mut message = if !file_refs.is_empty()
        && job.outbound_email.is_none()
        && founder_email_reply_message_key(job).is_none()
        && !is_founder_or_owner_email_job(job)
    {
        format!(
            "Die Aufgabe bleibt offen, weil erwartete dauerhafte Datei-Artefakte fehlen oder nicht nachweisbar sind: {}",
            clip_text(err, 240)
        )
    } else {
        format!(
            "Der Review hat den Entwurf nicht selbst versendet. Die Aufgabe bleibt offen, weil noch kein akzeptiertes Outbound-E-Mail-Artefakt existiert: {}",
            clip_text(err, 240)
        )
    };
    if let Some(inbound_key) = founder_email_reply_message_key(job) {
        message.push_str(&format!(
            "\n\nNaechster Schritt fuer den Agent-Run: Sende die freigegebene Antwort selbst mit `ctox channel founder-reply --message-key {}` und exakt dem bereits freigegebenen Mailtext als `--body`. Aendere To, CC, Betreff und Body nicht, sonst passt die Review-Freigabe nicht mehr. Danach pruefe, dass `communication_messages` fuer diesen Mail-Thread eine outbound email mit `status='accepted'` enthaelt.",
            inbound_key
        ));
    } else if let Some(action) = job.outbound_email.as_ref() {
        let to_flags = action
            .to
            .iter()
            .map(|value| format!(" --to {}", shell_quote(value)))
            .collect::<String>();
        let cc_flags = action
            .cc
            .iter()
            .map(|value| format!(" --cc {}", shell_quote(value)))
            .collect::<String>();
        let approved_body_block = approved_body.trim();
        message.push_str(&format!(
            "\n\nNaechster Schritt fuer den Worker: Die Review-Freigabe fuer exakt diesen Body und diese Empfaenger ist bereits persistiert. Fuehre keine DB- oder Code-Forensik aus und erstelle keine Review-Zeilen manuell. Sende genau den freigegebenen Text mit genau diesem Befehl:\n\nBODY=$(cat <<'CTOX_REVIEWED_BODY'\n{}\nCTOX_REVIEWED_BODY\n)\nctox channel send --channel email --account-key {} --thread-key {} --subject {}{}{} --reviewed-founder-send --body \"$BODY\"\n\nAendere Body, Empfaenger, CC oder Betreff nicht. Wenn der Befehl fehlschlaegt, melde exakt die Fehlermeldung und stoppe. Wenn er erfolgreich ist, pruefe, dass `communication_messages` fuer thread_key `{}` eine outbound email mit `status='accepted'` enthaelt.",
            approved_body_block,
            shell_quote(&action.account_key),
            shell_quote(&action.thread_key),
            shell_quote(&action.subject),
            to_flags,
            cc_flags,
            action.thread_key
        ));
    } else {
        if file_refs.is_empty() {
            message.push_str(
                "\n\nNaechster Schritt fuer den Agent-Run: Fuehre die verlangte Aktion selbst aus und speichere das Ergebnis als dauerhaftes Artefakt. Danach pruefe das Artefakt explizit, bevor du die Aufgabe als abgeschlossen meldest.",
            );
        } else {
            message.push_str(
                "\n\nHARNESS FEEDBACK\nProblem: Du hast die Aufgabe als fertig behandelt, aber der Harness konnte die erwarteten Datei-Artefakte nicht als Ergebnis dieses Turns nachweisen. Eine Textantwort, ein Plan oder ein Codeblock reicht hier nicht.\n\nREQUIRED ARTIFACTS\nDiese Pfade muessen als regulaere Dateien existieren und, wenn unten als stale markiert, in diesem Turn aktualisiert werden, bevor du Abschluss behauptest:",
            );
            if is_terminal_bench_controller_artifact_job(job) {
                if let Some(run_dir) = terminal_bench_run_dir_from_artifact_paths(&file_refs) {
                    message.push_str(&format!(
                        "\n\nCURRENT TERMINAL-BENCH RUN SCOPE\nDer einzige gueltige RUN_DIR fuer diesen Queue-Run ist:\n{}\n\nWenn du Dateien in einem anderen Terminal-Bench-Run-Ordner erzeugt oder gelesen hast, war das ein Wrong-Run-Fehler. Verwende keine controller-prompt.md oder durable Dateien aus alten Runs. Arbeite jetzt nur in diesem RUN_DIR weiter.",
                        run_dir
                    ));
                }
            }
            for artifact in expected_file_artifacts {
                let path = artifact.primary_key;
                let diagnostic = workspace_file_artifact_diagnostic_for_expected(
                    root,
                    job,
                    &ArtifactRef {
                        kind: ArtifactKind::WorkspaceFile,
                        primary_key: path.clone(),
                        expected_terminal_state: artifact.expected_terminal_state,
                    },
                );
                message.push_str(&format!("\n- {} [{}]", path, diagnostic));
            }
            message.push_str(
                "\n\nNEXT ACTION\n1. Fuehre jetzt einen Terminal-/Shell-Toolcall aus. Schreibe nicht nur, was du tun wuerdest.\n2. Erzeuge oder aktualisiere genau diese Artefakte als regulaere Dateien. Wenn `test -d '<pfad>'` fuer einen erforderlichen Pfad erfolgreich ist, ist genau das der Fehler: verschiebe oder entferne dieses Verzeichnis und schreibe die Datei an denselben Pfad. Schreibe die Artefakte nicht in `<pfad>/...`.\n3. Fuer stale markierte Dateien reicht vorhandene Existenz nicht: schreibe einen truthful checkpoint oder fuehre mindestens eine inhaltlich korrekte Aktualisierung im aktuellen RUN_DIR aus.\n4. Pruefe jeden Pfad mit `test -f '<pfad>'`.\n5. Wenn ein Artefakt absichtlich leer sein darf, ist Existenz genug; sonst schreibe den geforderten Inhalt hinein.\n6. Antworte erst danach mit einer kurzen Ergebniszusammenfassung.\n\nORIGINAL TASK\nDer urspruengliche Auftrag bleibt aktiv und ist weiterhin der fachliche Inhalt fuer die Artefakte:\n",
            );
            message.push_str(&clip_text(&job.prompt, 6000));
            message.push_str(
                "\n\nEXIT GATE\nDu darfst diese Aufgabe erst als erledigt behandeln, wenn alle oben genannten `test -f` Pruefungen erfolgreich sind und alle stale markierten Dateien in diesem Turn aktualisiert wurden.",
            );
        }
    }
    message
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn job_outcome_entity_id(job: &QueuedPrompt) -> String {
    job.ticket_self_work_id
        .clone()
        .or_else(|| job.leased_message_keys.first().cloned())
        .unwrap_or_else(|| format!("job:{}", channels::stable_digest(&job.source_label)))
}

fn outcome_witness_retry_route_status(root: &Path, job: &QueuedPrompt) -> &'static str {
    match outcome_witness_rejection_count(root, job) {
        Ok(count) if count >= review_checkpoint_requeue_block_threshold() => "blocked",
        Ok(_) | Err(_) => "review_rework",
    }
}

fn outcome_witness_retry_route_status_for_job(root: &Path, job: &QueuedPrompt) -> &'static str {
    if job.source_label == "review-feedback" && is_terminal_bench_controller_artifact_job(job) {
        return "pending";
    }
    if is_terminal_bench_controller_artifact_job(job) {
        return "review_rework";
    }
    outcome_witness_retry_route_status(root, job)
}

fn should_queue_artifact_outcome_recovery(job: &QueuedPrompt) -> bool {
    if declared_workspace_file_artifacts_for_job(job).is_empty() {
        return false;
    }
    !(job.source_label == "review-feedback" && is_terminal_bench_controller_artifact_job(job))
}

fn outcome_witness_rejection_count(root: &Path, job: &QueuedPrompt) -> Result<usize> {
    let conn = channels::open_channel_db(&crate::paths::core_db(&root))?;
    let entity_id = job_outcome_entity_id(job);
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_transition_proofs
        WHERE entity_id = ?1
          AND accepted = 0
          AND (
            violation_codes_json LIKE '%WP-Outcome-Missing%'
            OR violation_codes_json LIKE '%WP-Outcome-Wrong-State%'
          )
        "#,
        params![entity_id],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as usize)
}

fn enforce_review_checkpoint_feedback_transition(
    root: &Path,
    work_id: &str,
    outcome: &review::ReviewOutcome,
) -> Result<String> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let mut metadata = BTreeMap::new();
    metadata.insert("review_checkpoint".to_string(), "true".to_string());
    metadata.insert("feedback_owner".to_string(), "main_agent".to_string());
    metadata.insert("feedback_target_entity_id".to_string(), work_id.to_string());
    metadata.insert("spawns_review_owned_work".to_string(), "false".to_string());
    metadata.insert(
        "review_verdict".to_string(),
        outcome.verdict.as_gate_label().to_string(),
    );

    let proof = enforce_core_transition(
        &conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::WorkItem,
            entity_id: work_id.to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::AwaitingReview,
            to_state: CoreState::ReworkRequired,
            event: CoreEvent::RequireRework,
            actor: "ctox-completion-review".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some(review_checkpoint_audit_key(work_id, outcome)),
                ..CoreEvidenceRefs::default()
            },
            metadata,
        },
    )?;
    Ok(proof.proof_id)
}

fn review_checkpoint_audit_key(work_id: &str, outcome: &review::ReviewOutcome) -> String {
    use sha2::Digest;

    let mut hasher = sha2::Sha256::new();
    hasher.update(b"ctox-review-checkpoint-v1");
    hasher.update(work_id.as_bytes());
    hasher.update(outcome.verdict.as_gate_label().as_bytes());
    hasher.update(outcome.summary.as_bytes());
    hasher.update(outcome.report.as_bytes());
    for gate in &outcome.failed_gates {
        hasher.update(gate.as_bytes());
        hasher.update(b"\0");
    }
    format!("review-checkpoint-{:x}", hasher.finalize())
}

/// Convergence threshold for the lightweight rewrite-only loop. Defaults to
/// `DEFAULT_REWRITE_FAILURE_THRESHOLD` and is overridable via the
/// `CTOX_MISSION_REWRITE_FAILURE_THRESHOLD` env var. Non-numeric or
/// non-positive overrides fall back to the default; oversized values are
/// capped so the safety proof stays operationally meaningful.
fn mission_rewrite_failure_threshold() -> i64 {
    match std::env::var("CTOX_MISSION_REWRITE_FAILURE_THRESHOLD") {
        Ok(value) => match value.trim().parse::<i64>() {
            Ok(parsed) if parsed > 0 => parsed.min(MAX_REWRITE_FAILURE_THRESHOLD),
            _ => DEFAULT_REWRITE_FAILURE_THRESHOLD,
        },
        Err(_) => DEFAULT_REWRITE_FAILURE_THRESHOLD,
    }
}

fn mission_agent_failure_threshold() -> i64 {
    match std::env::var("CTOX_MISSION_AGENT_FAILURE_THRESHOLD") {
        Ok(value) => match value.trim().parse::<i64>() {
            Ok(parsed) if parsed > 0 => parsed.min(MAX_AGENT_FAILURE_THRESHOLD),
            _ => DEFAULT_AGENT_FAILURE_THRESHOLD,
        },
        Err(_) => DEFAULT_AGENT_FAILURE_THRESHOLD,
    }
}

fn timeout_auto_retry_enabled() -> bool {
    std::env::var("CTOX_TIMEOUT_AUTO_RETRY")
        .ok()
        .and_then(|value| parse_boolish(&value))
        .unwrap_or(false)
}

fn failed_worker_route_status(
    agent_failure_threshold_hit: bool,
    timeout_worker_message: bool,
    retry_worker_message: bool,
) -> &'static str {
    if agent_failure_threshold_hit || (timeout_worker_message && !retry_worker_message) {
        "blocked"
    } else if retry_worker_message {
        "pending"
    } else {
        "failed"
    }
}

fn parse_boolish(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Build the lightweight rewrite-only prompt body. The agent receives the
/// prior outbound body verbatim (between fenced markers), the structured
/// list of rewrite findings with corrective actions, and a strict
/// "reply-with-body-only" instruction so the review checkpoint can approve
/// the corrected body against the same outbound action without re-deriving
/// recipients/subjects from scratch.
fn build_review_rewrite_prompt(
    prior_body: &str,
    findings: &[RewriteFinding],
    anchor_message_key: Option<&str>,
) -> String {
    let mut numbered = String::new();
    for (idx, finding) in findings.iter().enumerate() {
        let id = if finding.id.trim().is_empty() {
            format!("f{}", idx + 1)
        } else {
            finding.id.trim().to_string()
        };
        numbered.push_str(&format!(
            "{}. [{}] evidence: {} | corrective_action: {}\n",
            idx + 1,
            id,
            finding.evidence.trim(),
            finding.corrective_action.trim()
        ));
    }
    if numbered.is_empty() {
        numbered.push_str("(none)\n");
    }
    let anchor_note = match anchor_message_key {
        Some(key) if !key.trim().is_empty() => {
            format!("\nKontext-Anchor: {}\n", key.trim())
        }
        _ => String::new(),
    };
    format!(
        "Du erhältst den vorigen Body einer reviewed founder send Mail und eine Liste von Wording-/Style-Findings.\n\
Erstelle den korrigierten Body — alles andere bleibt unverändert.\n\
{anchor_note}\n\
Vorheriger Body (zwischen ====):\n\
====\n\
{prior_body}\n\
====\n\
\n\
Findings (jeweils mit corrective_action):\n\
{numbered}\n\
Reply: nur der korrigierte Body. Keine eigenen \"An:\", \"Betreff:\" oder \"From:\"-Zeilen. Keine Erläuterung.\n",
        anchor_note = anchor_note,
        prior_body = prior_body.trim(),
        numbered = numbered.trim_end(),
    )
}

fn build_review_feedback_retry_prompt(
    job: &QueuedPrompt,
    outcome: &review::ReviewOutcome,
    prior_reply: &str,
) -> String {
    let failed_gates_block = render_review_feedback_block(
        &outcome.failed_gates,
        "The result is not ready yet. Use the review evidence below to fix the actual missing outcome before finishing.",
        6,
    );
    let findings_block = render_review_feedback_block(
        &outcome.semantic_findings,
        "The review did not provide a clean finding sentence. Re-read the current task, current thread, and expected artifact before continuing.",
        8,
    );
    let open_items_block = render_review_feedback_block(
        &outcome.open_items,
        "Fix the missing work, then submit the corrected result through the same reviewed path.",
        8,
    );
    let evidence_block = render_review_feedback_block(
        &outcome.evidence,
        "No extra evidence was provided by the review. Reconstruct the evidence from CTOX state before finishing.",
        8,
    );
    let send_instruction = if job.outbound_email.is_some() {
        "\nFor this owner/founder email task, the Review Gate only checks and records approval. It does not send the email. After your corrected draft is approved, you must run the reviewed send command yourself and verify that CTOX has an outbound email artifact with status `accepted`. Do not mark the task complete until that durable accepted artifact exists.\n"
    } else {
        ""
    };
    format!(
        "The external CTOX Review Gate checked your last result and found that it is not complete yet. Continue the same task now; do not create a subtask, queue task, or self-rework item.\n\n\
Review summary: {}\n\n\
What is wrong:\n\
{}\n\n\
Evidence:\n\
{}\n\n\
Required next actions:\n\
{}\n\n\
Additional findings:\n\
{}\n\
{send_instruction}\n\
Your previous result is below. Treat it as a draft, not as proof of completion.\n\
==== previous result ====\n\
{}\n\
==== end previous result ====\n\n\
Now continue the task. Produce the corrected artifact or perform the required CTOX command yourself. Do not describe completion unless the durable artifact exists.",
        clip_text(&outcome.summary, 280),
        failed_gates_block,
        evidence_block,
        open_items_block,
        findings_block,
        prior_reply.trim(),
    )
}

/// Enqueue a high-priority rework slice on the same thread as the rejected
/// slice. Only the structured verdict summary is forwarded; the full review
/// run remains external and should not leak back into executor prompts.
fn enqueue_review_rework(
    root: &Path,
    job: &QueuedPrompt,
    outcome: &review::ReviewOutcome,
) -> Result<String> {
    if let Some(existing) = find_superseding_corrective_queue_task(
        root,
        job.thread_key.as_deref(),
        job.workspace_root.as_deref(),
        &["review rework"],
    )? {
        anyhow::bail!(
            "superseded by runnable corrective work already in queue: {} ({})",
            existing.title,
            existing.message_key
        );
    }
    let summary_line = clip_text(&outcome.summary, 220);
    let preview = clip_text(&job.preview, 80);
    let title = format!(
        "Review rework: {} ({})",
        if preview.is_empty() {
            "(no preview)"
        } else {
            preview.as_str()
        },
        outcome.verdict.as_gate_label()
    );
    let failed_gates_block = if outcome.failed_gates.is_empty() {
        "- none".to_string()
    } else {
        outcome
            .failed_gates
            .iter()
            .take(6)
            .map(|item| format!("- {}", item.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let findings_block = if outcome.semantic_findings.is_empty() {
        "- none".to_string()
    } else {
        outcome
            .semantic_findings
            .iter()
            .take(8)
            .map(|item| format!("- {}", item.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let open_items_block = if outcome.open_items.is_empty() {
        "- none".to_string()
    } else {
        outcome
            .open_items
            .iter()
            .take(8)
            .map(|item| format!("- {}", item.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let prompt = format!(
        "An external CTOX review run rejected the previous slice.\n\n\
Verdict: {}\n\
Mission state: {}\n\
Review summary: {}\n\
\n\
Failed gates:\n\
{}\n\
\n\
Semantic findings:\n\
{}\n\
\n\
Open items:\n\
{}\n\
\n\
Address the failed gates and open items surfaced by the external review. \
Start by checking the persisted review verdict and evidence for this conversation or thread. \
Do not start unrelated work. Either fix the gaps and verify them with direct checks, \
or prove the review wrong with stronger evidence.",
        outcome.verdict.as_gate_label(),
        outcome.mission_state,
        summary_line,
        failed_gates_block,
        findings_block,
        open_items_block,
    );
    // Keep the rework on the original thread when one exists so the executor
    // sees its own prior conversation context. Synthesize a fallback thread
    // from the source label when the original came in without one (rare —
    // mostly non-TUI background sources).
    let thread_key = job
        .thread_key
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| format!("review-rework:{}", job.source_label));
    let view = create_self_work_backed_queue_task(
        root,
        DurableSelfWorkQueueRequest {
            kind: "review-rework".to_string(),
            title,
            prompt,
            thread_key,
            workspace_root: job.workspace_root.clone(),
            priority: "high".to_string(),
            suggested_skill: job
                .suggested_skill
                .clone()
                .or_else(|| Some("follow-up-orchestrator".to_string())),
            parent_message_key: job.leased_message_keys.first().cloned(),
            metadata: serde_json::json!({
                "dedupe_key": format!(
                    "review-rework:{}:{}:{}",
                    job.thread_key.as_deref().unwrap_or(job.source_label.as_str()),
                    outcome.verdict.as_gate_label(),
                    clip_text(&summary_line, 80),
                ),
                "origin_source_label": job.source_label,
            }),
        },
    )?;
    Ok(view.title)
}

fn handle_actionable_completion_review_rejection(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    job: &QueuedPrompt,
    outcome: &review::ReviewOutcome,
) -> CompletionReviewDisposition {
    if let Some(work_id) = resolve_review_rejection_target_self_work_id(root, job) {
        push_event(
            state,
            format!(
                "Review fail for {} will resume durable self-work {} instead of nesting review-rework",
                job.source_label, work_id
            ),
        );
        return CompletionReviewDisposition::RequeueSelfWork {
            work_id,
            summary: outcome.summary.clone(),
        };
    }
    match enqueue_review_rework(root, job, outcome) {
        Ok(rework_title) => push_event(state, format!("Review rework enqueued: {rework_title}")),
        Err(err) => push_event(
            state,
            format!(
                "Review rework enqueue failed for {}: {}",
                job.source_label, err
            ),
        ),
    }
    CompletionReviewDisposition::Hold {
        summary: outcome.summary.clone(),
    }
}

fn render_review_feedback_block(items: &[String], fallback: &str, limit: usize) -> String {
    let mut rendered = items
        .iter()
        .filter_map(|item| naturalize_review_feedback_item(item))
        .take(limit)
        .collect::<Vec<_>>();
    if rendered.is_empty() {
        rendered.push(fallback.to_string());
    }
    rendered
        .into_iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn naturalize_review_feedback_item(item: &str) -> Option<String> {
    let trimmed = item.trim().trim_start_matches('-').trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }

    let lowered = trimmed.to_ascii_lowercase();
    let mapped = if lowered == "missing_deliverable" || lowered.contains("missing deliverable") {
        Some("Ein ausdruecklich angefordertes Ergebnis fehlt; es muss erstellt oder beschafft werden, bevor erneut geantwortet wird.")
    } else if lowered == "unbacked_commitment" || lowered.contains("unbacked commitment") {
        Some("Eine zugesagte Frist oder Lieferung ist nicht durch einen konkreten Termin, eine Folgeaufgabe oder ueberpruefbare Arbeit abgesichert.")
    } else if lowered.contains("founder_communication") || lowered.contains("founder communication")
    {
        Some("Die Antwort erfuellt die Founder-Kommunikation nicht: aktuelle Frage, Empfaengerlogik oder Kontextbezug sind nicht sauber getroffen.")
    } else if lowered.contains("owner_visible_claim") || lowered.contains("owner visible claim") {
        Some("Eine Aussage an den Owner ist nicht ausreichend durch ueberpruefbare Arbeit oder sichtbare Evidenz belegt.")
    } else if lowered.contains("closure_claim") || lowered.contains("closure claim") {
        Some("Der Entwurf behauptet Abschluss oder Fortschritt, ohne dass der Abschluss ausreichend belegt ist.")
    } else if lowered.contains("artifact action") {
        None
    } else {
        None
    };
    if let Some(mapped) = mapped {
        return Some(mapped.to_string());
    }

    let mut text = trimmed.to_string();
    let replacements = [
        ("NO-SEND", "nicht sendbarer Entwurf"),
        ("no-send", "nicht sendbarer Entwurf"),
        ("route state", "interne Zustandsmeldung"),
        ("route_status", "interne Zustandsmeldung"),
        ("runtime status", "interner Laufzeitstatus"),
        ("runtime proof", "interner Laufzeitnachweis"),
        ("claim list", "interne Behauptungsliste"),
        ("ctox channel send", "allgemeiner Versandkanal"),
        (
            "founder_or_owner_outbound_email_draft",
            "Founder- oder Owner-Mailentwurf",
        ),
        (
            "reviewed founder communication path",
            "gepruefter Founder-Mail-Pfad",
        ),
        ("reviewed-send-proof", "gepruefter Versandnachweis"),
    ];
    for (needle, replacement) in replacements {
        text = text.replace(needle, replacement);
    }

    if looks_like_internal_label(&text) {
        return Some(
            "Eine erforderliche Bedingung ist nicht erfuellt; nutze die Befunde und Evidenz darunter, um die inhaltliche Nacharbeit zu erledigen."
                .to_string(),
        );
    }

    while text.contains("  ") {
        text = text.replace("  ", " ");
    }
    Some(clip_text(&text, 280))
}

fn looks_like_internal_label(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.contains("::") || trimmed.contains("=>") {
        return true;
    }
    if trimmed.contains('_') {
        return true;
    }
    let codeish = [
        "artifact",
        "metadata",
        "sqlite",
        "table ",
        "prompt",
        "system message",
    ];
    let lowered = trimmed.to_ascii_lowercase();
    codeish.iter().any(|needle| lowered.contains(needle))
}

fn enqueue_review_stale_refresh(
    root: &Path,
    job: &QueuedPrompt,
    outcome: &review::ReviewOutcome,
) -> Result<String> {
    if let Some(existing) = find_superseding_corrective_queue_task(
        root,
        job.thread_key.as_deref(),
        job.workspace_root.as_deref(),
        &["stale refresh", "stale consolidate", "review stale"],
    )? {
        anyhow::bail!(
            "superseded by runnable stale-refresh work already in queue: {} ({})",
            existing.title,
            existing.message_key
        );
    }
    let summary_line = clip_text(&outcome.summary, 220);
    let preview = clip_text(&job.preview, 80);
    let stale_categories = outcome
        .categorized_findings
        .iter()
        .filter(|finding| finding.category.is_stale())
        .map(|finding| {
            format!(
                "- {}: {} -> {}",
                finding.category.as_str(),
                clip_text(&finding.evidence, 160),
                clip_text(&finding.corrective_action, 160)
            )
        })
        .collect::<Vec<_>>();
    let stale_block = if stale_categories.is_empty() {
        "- stale context changed; reload the current thread and queue state before continuing"
            .to_string()
    } else {
        stale_categories.join("\n")
    };
    let thread_key = job
        .thread_key
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| format!("review-stale:{}", job.source_label));
    let title = format!(
        "Review stale refresh: {}",
        if preview.is_empty() {
            "(no preview)"
        } else {
            preview.as_str()
        }
    );
    let prompt = format!(
        "An external CTOX review found that the previous slice is stale rather than merely wrong.\n\n\
Review summary: {summary_line}\n\n\
Stale findings:\n\
{stale_block}\n\n\
Reload the current thread, inbound messages, queue rows, and active mission/strategy before drafting or closing anything. \
If the prior draft is obsolete, cancel or supersede it. If multiple queue items now describe the same changed world state, consolidate them. \
Only produce a new draft after the current state is reflected in durable runtime records."
    );
    let view = create_self_work_backed_queue_task(
        root,
        DurableSelfWorkQueueRequest {
            kind: "review-stale-refresh".to_string(),
            title,
            prompt,
            thread_key,
            workspace_root: job.workspace_root.clone(),
            priority: "high".to_string(),
            suggested_skill: job
                .suggested_skill
                .clone()
                .or_else(|| Some("follow-up-orchestrator".to_string())),
            parent_message_key: job.leased_message_keys.first().cloned(),
            metadata: serde_json::json!({
                "dedupe_key": format!(
                    "review-stale-refresh:{}:{}",
                    job.thread_key.as_deref().unwrap_or(job.source_label.as_str()),
                    clip_text(&summary_line, 80),
                ),
                "origin_source_label": job.source_label,
            }),
        },
    )?;
    Ok(view.title)
}

fn enqueue_founder_communication_rework(
    root: &Path,
    job: &QueuedPrompt,
    inbound_message_key: &str,
    outcome: &review::ReviewOutcome,
) -> Result<String> {
    let summary_line = clip_text(&outcome.summary, 220);
    let preview = clip_text(&job.preview, 80);
    let title = format!(
        "Founder communication rework: {} ({})",
        if preview.is_empty() {
            "(no preview)"
        } else {
            preview.as_str()
        },
        outcome.verdict.as_gate_label()
    );
    let failed_gates_block = render_review_feedback_block(
        &outcome.failed_gates,
        "Die Antwort ist noch nicht sendereif; nutze Befunde und Evidenz fuer die konkrete Nacharbeit.",
        6,
    );
    let findings_block = render_review_feedback_block(
        &outcome.semantic_findings,
        "Der Review hat keinen klaren Befundtext geliefert; pruefe den aktuellen Thread und die verlangten Ergebnisse erneut.",
        8,
    );
    let open_items_block = render_review_feedback_block(
        &outcome.open_items,
        "Pruefe die aktuelle Founder-Mail, erledige fehlende Arbeit, und schreibe erst danach eine sendefertige Antwort.",
        8,
    );
    let evidence_block = render_review_feedback_block(
        &outcome.evidence,
        "Keine belastbare Evidenz im Reviewtext; rekonstruiere sie aus dem aktuellen Mailthread und der Runtime-Historie, bevor du antwortest.",
        8,
    );
    let thread_key = job
        .thread_key
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("founder-rework:{}", job.source_label));
    let prompt = format!(
        "Der Review hat die letzte Founder-/Owner-Antwort blockiert.\n\n\
Wichtig: Behebe zuerst den inhaltlichen Grund. Wenn ein Ergebnis fehlt, erstelle oder beschaffe es. Wenn Empfaenger, Kopie, Threadbezug oder Kontext falsch waren, korrigiere die Antwortlogik. Reines Umformulieren reicht nur dann, wenn der Review ausdruecklich nur die Form beanstandet.\n\n\
Review-Kurzfassung: {}\n\n\
Was nicht passt:\n\
{}\n\n\
Evidenz aus dem Review:\n\
{}\n\n\
Was jetzt zu tun ist:\n\
{}\n\n\
Weitere Befunde:\n\
{}\n\n\
Erwartete Ausgabe:\n\
- Beende diesen Arbeitsschritt mit genau der E-Mail, die im bestehenden Thread an Founder oder Owner gehen soll.\n\
- Keine internen Statusberichte, keine Arbeitsnotizen, keine Tool- oder Tabellenbegriffe in der Antwort.\n\
- Umgehe den geprueften E-Mail-Pfad nicht; der Versand erfolgt erst nach der anschliessenden Pruefung.\n",
        summary_line,
        failed_gates_block,
        evidence_block,
        open_items_block,
        findings_block,
    );
    let view = create_self_work_backed_queue_task(
        root,
        DurableSelfWorkQueueRequest {
            kind: FOUNDER_COMMUNICATION_REWORK_KIND.to_string(),
            title,
            prompt,
            thread_key: thread_key.clone(),
            workspace_root: job.workspace_root.clone(),
            priority: "urgent".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            parent_message_key: Some(inbound_message_key.to_string()),
            metadata: serde_json::json!({
                "thread_key": thread_key,
                "workspace_root": job.workspace_root.clone(),
                "priority": "urgent",
                "inbound_message_key": inbound_message_key,
                "dedupe_key": format!("founder-communication-rework:{inbound_message_key}"),
                "origin_source_label": job.source_label,
            }),
        },
    )?;
    Ok(view.title)
}

fn start_channel_router(root: std::path::PathBuf, state: Arc<Mutex<SharedState>>) {
    thread::spawn(move || loop {
        if let Err(err) = route_external_messages(&root, &state) {
            push_event(&state, format!("Channel route failed: {err}"));
        }
        thread::sleep(Duration::from_secs(CHANNEL_ROUTER_POLL_SECS));
    });
}

fn start_channel_syncer(root: std::path::PathBuf) {
    thread::spawn(move || loop {
        let settings = live_service_settings(&root);
        sync_configured_channels(&root, &settings);
        thread::sleep(Duration::from_secs(channel_sync_poll_secs(&settings)));
    });
}

fn channel_sync_poll_secs(settings: &BTreeMap<String, String>) -> u64 {
    settings
        .get("CTOX_CHANNEL_SYNC_POLL_SECS")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|value| value.clamp(30, 900))
        .unwrap_or(CHANNEL_SYNC_POLL_SECS)
}

fn start_mission_maintenance_loop(root: std::path::PathBuf, state: Arc<Mutex<SharedState>>) {
    thread::spawn(move || {
        loop {
            // Emit any due plan steps first so auto-advancing plans keep moving
            // without requiring an explicit `ctox plan tick` call.
            if let Err(err) = plan::emit_due_steps(&root) {
                push_event(&state, format!("Plan emitter failed: {err}"));
            }
            // Autonomy-level dispatch:
            //   progressive -> drain any open approval-gate so plans keep
            //                  moving without human sign-off;
            //   balanced / defensive -> run the reminder sweep that pings
            //                  the owner through the configured channels
            //                  and closes gates on structured email replies.
            let level = crate::autonomy::AutonomyLevel::from_root(&root);
            if level.auto_closes_gates() {
                match auto_close_pending_approval_gates(&root) {
                    Ok(count) if count > 0 => push_event(
                        &state,
                        format!(
                            "Autonomy progressive: closed {count} pending approval-gate self-work item(s)"
                        ),
                    ),
                    Err(err) => {
                        push_event(&state, format!("Autonomy progressive sweep failed: {err}"))
                    }
                    _ => {}
                }
            } else {
                match crate::mission::approval_nag::sweep(&root) {
                    Ok(summary) => {
                        if summary.sent > 0
                            || summary.scheduled > 0
                            || summary.replies_processed > 0
                            || summary.completed > 0
                        {
                            push_event(
                                &state,
                                format!(
                                    "Approval nag: scheduled={} sent={} replies={} completed={}",
                                    summary.scheduled,
                                    summary.sent,
                                    summary.replies_processed,
                                    summary.completed
                                ),
                            );
                        }
                    }
                    Err(err) => push_event(&state, format!("Approval nag sweep failed: {err}")),
                }
            }
            thread::sleep(Duration::from_secs(MISSION_MAINTENANCE_POLL_SECS));
        }
    });
}

fn start_harness_audit_watcher(root: std::path::PathBuf, state: Arc<Mutex<SharedState>>) {
    // Periodically synthesizes a harness-mining brief and persists confirmed
    // findings to ctox_hm_findings via the 2-tick gate. Read-only against the
    // domain tables; only writes to ctox_hm_findings + ctox_hm_audit_runs, so
    // a failure here cannot poison the runtime store.
    thread::spawn(move || {
        // Initial offset so the first tick does not collide with the boot
        // burst (channel router + mission maintenance + supervisor are all
        // hammering the DB in the first 30s).
        thread::sleep(Duration::from_secs(60));
        loop {
            match harness_audit_tick_once(&root) {
                Ok(summary) => {
                    if summary.recorded > 0 || summary.confirmed > 0 {
                        push_event(
                            &state,
                            format!(
                                "Harness audit tick: recorded={}, confirmed={}, stale={} (run {})",
                                summary.recorded, summary.confirmed, summary.stale, summary.run_id
                            ),
                        );
                    }
                }
                Err(err) => {
                    push_event(&state, format!("Harness audit tick failed: {err}"));
                }
            }
            thread::sleep(Duration::from_secs(HARNESS_AUDIT_TICK_SECS));
        }
    });
}

struct HarnessAuditTickSummary {
    run_id: String,
    recorded: i64,
    confirmed: i64,
    stale: i64,
}

fn harness_audit_tick_once(root: &Path) -> Result<HarnessAuditTickSummary> {
    use crate::service::harness_mining::{brief, findings, now_iso_z};
    let db_path = crate::paths::core_db(&root);
    let conn = Connection::open(&db_path)
        .with_context(|| format!("audit tick: open db {}", db_path.display()))?;
    let report = findings::run_audit_tick(&conn, &brief::Options::default(), &now_iso_z())?;
    Ok(HarnessAuditTickSummary {
        run_id: report.run_id,
        recorded: report.recorded,
        confirmed: report.confirmed,
        stale: report.stale,
    })
}

/// Close every open `approval-gate` self-work item. Runs only when the
/// active autonomy level is `progressive` for unattended continuous
/// operation. Returns the count of items closed so callers can log it.
fn auto_close_pending_approval_gates(root: &Path) -> Result<usize> {
    // Limit is generous; the sweep runs every mission-watcher tick so a
    // slow backlog still drains over a few iterations.
    let pending = tickets::list_ticket_self_work_items(root, None, Some("open"), 256)?;
    let mut closed = 0usize;
    for item in pending {
        if item.kind == "approval-gate" {
            tickets::set_ticket_self_work_state(root, &item.work_id, "closed")?;
            closed += 1;
        }
    }
    Ok(closed)
}

fn live_service_settings(root: &Path) -> BTreeMap<String, String> {
    let mut settings = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    for (key, value) in env::vars() {
        if (!key.starts_with("CTOX_") && !key.starts_with("CTO_"))
            || crate::inference::runtime_state::is_runtime_state_key(&key)
        {
            continue;
        }
        settings.insert(key, value);
    }
    let _ = channels::merge_owner_profile_settings(root, &mut settings);
    settings
}

fn active_agent_loop_in_progress(state: &Arc<Mutex<SharedState>>) -> bool {
    let shared = lock_shared_state(state);
    shared.busy
}

fn route_external_messages(root: &Path, state: &Arc<Mutex<SharedState>>) -> Result<()> {
    if queue_pressure_active(state) {
        return Ok(());
    }
    // The channel router runs on its own timer. It may not repair, lease, or
    // reprioritize external work while a worker is still inside a full
    // reasoning/tool/review loop; arbitration belongs after that loop ends.
    if active_agent_loop_in_progress(state) {
        return Ok(());
    }
    route_assigned_ticket_self_work(root, state)?;
    let settings = live_service_settings(root);
    let ticket_preflight_issues = run_ticket_dispatch_preflight(root, state, &settings);
    let ticket_dispatch_allowed = ticket_preflight_issues
        .iter()
        .all(|issue| issue.severity != "error");
    if let Err(err) = reconcile_ticket_runtime_state(root, state) {
        push_event(state, format!("Ticket reconciliation failed: {err}"));
    }
    let repaired_founder_messages = repair_stalled_founder_communications(root, state, &settings)?;
    if repaired_founder_messages > 0 {
        push_event(
            state,
            format!(
                "Repaired {} stalled founder communication(s) before routing",
                repaired_founder_messages
            ),
        );
    }
    let scheduled = schedule::emit_due_tasks(root)?;
    if scheduled.emitted_count > 0 {
        push_event(
            state,
            format!("Scheduled {} cron task(s)", scheduled.emitted_count),
        );
    }
    let ticket_sync_allowed_sources = if ticket_dispatch_allowed {
        sync_configured_tickets(root, state, &settings)
    } else {
        HashSet::new()
    };
    let bot_name = settings
        .get("CTO_MEETING_BOT_NAME")
        .cloned()
        .unwrap_or_else(|| "INF Yoda Notetaker".to_string());
    let mut leased =
        channels::lease_pending_inbound_messages(root, 16, CHANNEL_ROUTER_LEASE_OWNER)?;
    leased.sort_by_key(|message| {
        std::cmp::Reverse(source_label_dispatch_rank(&inbound_source_label(
            &settings, message,
        )))
    });
    let mut seen = HashSet::new();
    let mut duplicates = Vec::new();
    let mut blocked = Vec::new();
    let mut meeting_handled = Vec::new();
    let mut meeting_passive = Vec::new();
    let mut deferred_for_founder_rework = Vec::new();
    for message in leased {
        if let Some(reason) = blocked_inbound_reason(&message, &settings) {
            let mechanism_id = governance::mechanism_id_for_block_reason(&reason);
            let event_key = format!("blocked-inbound:{}", message.message_key);
            let _ = governance::record_event(
                root,
                governance::GovernanceEventRequest {
                    mechanism_id,
                    conversation_id: None,
                    severity: "warning",
                    reason: &reason,
                    action_taken: "blocked inbound message before it entered the active loop",
                    details: serde_json::json!({
                        "channel": message.channel.clone(),
                        "message_key": message.message_key.clone(),
                        "sender": display_inbound_sender(&message),
                    }),
                    idempotence_key: Some(&event_key),
                },
            );
            push_event(
                state,
                format!(
                    "Blocked {} inbound from {}: {}",
                    message.channel,
                    display_inbound_sender(&message),
                    reason
                ),
            );
            blocked.push(message.message_key.clone());
            continue;
        }
        let dedupe_key = inbound_dedupe_key(&message);
        if !seen.insert(dedupe_key) {
            duplicates.push(message.message_key.clone());
            continue;
        }
        if message.channel == "meeting"
            && message
                .metadata
                .get("source")
                .and_then(serde_json::Value::as_str)
                == Some("meeting_chat")
            && !message
                .metadata
                .get("is_mention")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        {
            meeting_passive.push(message.message_key.clone());
            continue;
        }
        // --- Meeting invitation intercept ---
        // If this is an email containing a complete, policy-allowed meeting
        // invitation, schedule the join and ack the message instead of routing
        // it to the agent. Blocked or incomplete invitations fall through for
        // normal agent review.
        if message.channel == "email" {
            let body = if !message.body_text.trim().is_empty() {
                message.body_text.trim()
            } else {
                ""
            };
            let meeting_urls = crate::communication::meeting_native::extract_meeting_urls(body);
            if !meeting_urls.is_empty() {
                if let Some(reason) = meeting_auto_join_policy_block(&settings, &message) {
                    push_event(
                        state,
                        format!(
                            "Meeting auto-join blocked for {}: {}",
                            display_inbound_sender(&message),
                            reason
                        ),
                    );
                } else {
                    let result = crate::communication::meeting_native::process_email_for_meetings(
                        root,
                        message.subject.trim(),
                        body,
                        &bot_name,
                    );
                    if let Ok(ref val) = result {
                        if val.get("action").and_then(serde_json::Value::as_str)
                            == Some("processed")
                        {
                            push_event(
                                state,
                                format!(
                                    "Meeting detected in email from {}: {}",
                                    display_inbound_sender(&message),
                                    meeting_urls.first().unwrap_or(&String::new()),
                                ),
                            );
                            meeting_handled.push(message.message_key.clone());
                            continue;
                        }
                    }
                }
            }
        }

        let prompt_body = if !message.body_text.trim().is_empty() {
            message.body_text.trim().to_string()
        } else if !message.preview.trim().is_empty() {
            message.preview.trim().to_string()
        } else if !message.subject.trim().is_empty() {
            message.subject.trim().to_string()
        } else {
            duplicates.push(message.message_key.clone());
            continue;
        };
        if message.channel == "tui" && is_non_work_tui_probe(&prompt_body) {
            push_event(
                state,
                format!(
                    "Ignored non-work TUI route from {}",
                    display_inbound_sender(&message)
                ),
            );
            meeting_passive.push(message.message_key.clone());
            continue;
        }
        let leased_message_key = message.message_key.clone();
        if is_founder_or_owner_inbound_message(&settings, &message) {
            if open_founder_communication_rework_for_inbound(root, &leased_message_key)? {
                deferred_for_founder_rework.push(leased_message_key);
                continue;
            }
        }
        let mut leased_message_keys = vec![leased_message_key.clone()];
        let mut source_label = inbound_source_label(&settings, &message);
        let founder_rework_inbound_key = if is_founder_communication_rework_queue_message(&message)
        {
            founder_rework_inbound_message_key(&message)
        } else {
            None
        };
        if founder_rework_inbound_key.is_some() {
            if let Some(inbound_key) = founder_rework_inbound_key.as_deref() {
                if !leased_message_keys.iter().any(|key| key == &inbound_key) {
                    leased_message_keys.push(inbound_key.to_string());
                }
            }
            if let Some(origin_source) = founder_rework_origin_source_label(&message) {
                source_label = origin_source;
            }
        }
        if leased_message_keys
            .iter()
            .any(|key| inflight_leased_message_key(state, key))
        {
            continue;
        }
        let prompt = if let Some(inbound_key) = founder_rework_inbound_key.as_deref() {
            render_founder_communication_rework_execution_prompt(
                root,
                &message,
                inbound_key,
                &prompt_body,
            )
        } else {
            enrich_inbound_prompt(root, &settings, &message, &prompt_body)
        };
        let goal = if let Some(inbound_key) = founder_rework_inbound_key.as_deref() {
            format!("Founder communication rework for {inbound_key}")
        } else {
            prompt_body.clone()
        };
        enqueue_prompt(
            root,
            state,
            QueuedPrompt {
                preview: preview_text(&prompt),
                source_label,
                goal,
                prompt,
                suggested_skill: suggested_skill_from_message(&message),
                leased_message_keys,
                leased_ticket_event_keys: ticket_event_key_from_metadata(&message.metadata)
                    .into_iter()
                    .collect(),
                thread_key: Some(execution_thread_key_for_inbound_message(
                    &settings, &message,
                )),
                workspace_root: message.workspace_root.clone(),
                ticket_self_work_id: ticket_self_work_id_from_metadata(&message.metadata),
                outbound_email: founder_outbound_action_from_metadata(&message.metadata),
                outbound_anchor: metadata_string(&message.metadata, "outbound_anchor"),
            },
            format!(
                "Queued {} inbound from {}",
                message.channel,
                if !message.sender_display.trim().is_empty() {
                    message.sender_display.trim()
                } else {
                    message.sender_address.trim()
                }
            ),
        );
    }
    if !duplicates.is_empty() {
        let _ = channels::ack_leased_messages(root, &duplicates, "duplicate");
    }
    if !blocked.is_empty() {
        let _ = channels::ack_leased_messages(root, &blocked, "blocked_sender");
    }
    if !meeting_handled.is_empty() {
        let _ = channels::ack_leased_messages(root, &meeting_handled, "meeting_scheduled");
    }
    if !meeting_passive.is_empty() {
        let _ = channels::ack_leased_messages(root, &meeting_passive, "handled");
    }
    if !deferred_for_founder_rework.is_empty() {
        let _ = channels::ack_leased_messages(root, &deferred_for_founder_rework, "review_rework");
    }
    if ticket_dispatch_allowed && !ticket_sync_allowed_sources.is_empty() {
        route_ticket_events(root, state, &ticket_sync_allowed_sources)?;
    }
    Ok(())
}

fn run_ticket_dispatch_preflight(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    settings: &BTreeMap<String, String>,
) -> Vec<tickets::TicketDispatchPreflightIssue> {
    let issues = tickets::preflight_configured_ticket_systems(root, settings);
    for issue in &issues {
        let idempotence_key = format!("ticket-preflight:{}:{}", issue.system, issue.code);
        let system = issue.system.clone();
        let code = issue.code.clone();
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "ticket_dispatch_preflight",
                conversation_id: None,
                severity: &issue.severity,
                reason: &issue.reason,
                action_taken: "skipped ticket sync and ticket event dispatch for this router cycle",
                details: serde_json::json!({
                    "system": system,
                    "code": code,
                }),
                idempotence_key: Some(&idempotence_key),
            },
        );
        push_event(
            state,
            format!(
                "Ticket dispatch preflight blocked {} [{}]: {}",
                issue.system, issue.code, issue.reason
            ),
        );
    }
    issues
}

fn reconcile_ticket_runtime_state(root: &Path, state: &Arc<Mutex<SharedState>>) -> Result<()> {
    let active_keys = {
        let shared = lock_shared_state(state);
        shared.leased_message_keys_inflight.clone()
    };
    let released_queue_leases =
        channels::release_stale_queue_task_leases(root, CHANNEL_ROUTER_LEASE_OWNER, &active_keys)?;
    if !released_queue_leases.is_empty() {
        let released_count = released_queue_leases.len();
        let idempotence_key = format!(
            "ticket-reconcile:released-queue:{}",
            normalize_token(&released_queue_leases.join(","))
        );
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "ticket_reconciliation",
                conversation_id: None,
                severity: "info",
                reason: "leased ticket-backed queue tasks had no active in-process worker or queued prompt",
                action_taken: "released stale queue task leases back to pending",
                details: serde_json::json!({
                    "released_message_keys": released_queue_leases.clone(),
                }),
                idempotence_key: Some(&idempotence_key),
            },
        );
        push_event(
            state,
            format!("Released {released_count} stale queue task lease(s)"),
        );
    }
    let released_leases =
        tickets::release_stale_ticket_event_leases(root, CHANNEL_ROUTER_LEASE_OWNER, &active_keys)?;
    if !released_leases.is_empty() {
        let released_count = released_leases.len();
        let idempotence_key = format!(
            "ticket-reconcile:released-leases:{}",
            normalize_token(&released_leases.join(","))
        );
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "ticket_reconciliation",
                conversation_id: None,
                severity: "info",
                reason: "leased ticket events had no active in-process worker or queued prompt",
                action_taken: "released stale ticket event leases back to pending",
                details: serde_json::json!({
                    "released_event_keys": released_leases.clone(),
                }),
                idempotence_key: Some(&idempotence_key),
            },
        );
        push_event(
            state,
            format!("Released {released_count} stale ticket event lease(s)"),
        );
    }

    let released_blocked = tickets::release_ready_blocked_ticket_events(root, 64)?;
    if !released_blocked.is_empty() {
        let released_count = released_blocked.len();
        let idempotence_key = format!(
            "ticket-reconcile:released-blocked:{}",
            normalize_token(&released_blocked.join(","))
        );
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "ticket_reconciliation",
                conversation_id: None,
                severity: "info",
                reason:
                    "blocked ticket events became preparable after knowledge/control state changed",
                action_taken: "released blocked ticket events back to pending",
                details: serde_json::json!({
                    "released_event_keys": released_blocked.clone(),
                }),
                idempotence_key: Some(&idempotence_key),
            },
        );
        push_event(
            state,
            format!("Released {released_count} previously blocked ticket event(s)"),
        );
    }
    Ok(())
}

fn route_assigned_ticket_self_work(root: &Path, state: &Arc<Mutex<SharedState>>) -> Result<()> {
    let mut items = tickets::list_ticket_self_work_items(root, None, Some("published"), 128)?;
    items.extend(tickets::list_ticket_self_work_items(
        root,
        None,
        Some("queued"),
        128,
    )?);
    for item in items {
        if item.assigned_to.as_deref() != Some("self") {
            continue;
        }
        if let Some(reason) = suppress_self_work_reason(root, &item)? {
            supersede_ticket_self_work_item(
                root,
                &item.work_id,
                &format!("Closed without routing because the work was superseded: {reason}"),
            );
            push_event(
                state,
                format!(
                    "Suppressed self-work {} [{}]: {}",
                    item.work_id, item.kind, reason
                ),
            );
            continue;
        }
        if let Some(created) = queue_ticket_self_work_item(root, &item)? {
            push_event(
                state,
                decorate_service_event_with_skill(
                    &format!(
                        "Queued self-work {} for active handling [{}]",
                        item.work_id, item.kind
                    ),
                    created.suggested_skill.as_deref(),
                ),
            );
        }
    }
    Ok(())
}

fn route_ticket_events(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    allowed_sources: &HashSet<String>,
) -> Result<()> {
    let leased = tickets::lease_pending_ticket_events_for_sources(
        root,
        16,
        CHANNEL_ROUTER_LEASE_OWNER,
        Some(allowed_sources),
    )?;
    if leased.is_empty() {
        return Ok(());
    }
    let mut duplicates = Vec::new();
    let mut blocked = Vec::new();
    for event in leased {
        if inflight_leased_message_key(state, &event.event_key) {
            continue;
        }
        let prepared = match tickets::prepare_ticket_event_for_prompt(root, &event.event_key) {
            Ok(prepared) => prepared,
            Err(err) => {
                let err: anyhow::Error = err;
                blocked.push(event.event_key.clone());
                let reason = clip_text(&err.to_string(), 180);
                let is_knowledge_gate = err.to_string().contains("ticket knowledge gate:");
                let mechanism_id = if is_knowledge_gate {
                    "ticket_knowledge_gate"
                } else {
                    "ticket_control_gate"
                };
                let action_taken = if is_knowledge_gate {
                    "blocked ticket event before active handling because required ticket knowledge was not yet available"
                } else {
                    "blocked ticket event before active handling because its control state was incomplete"
                };
                let idempotence_key = format!("blocked-ticket:{}", event.event_key);
                let _ = governance::record_event(
                    root,
                    governance::GovernanceEventRequest {
                        mechanism_id,
                        conversation_id: None,
                        severity: "warning",
                        reason: &reason,
                        action_taken,
                        details: serde_json::json!({
                            "event_key": event.event_key.clone(),
                            "ticket_key": event.ticket_key.clone(),
                            "event_type": event.event_type.clone(),
                            "source_system": event.source_system.clone(),
                        }),
                        idempotence_key: Some(&idempotence_key),
                    },
                );
                push_event(
                    state,
                    format!("Blocked ticket event {}: {}", event.event_key, reason),
                );
                continue;
            }
        };
        let prompt = render_ticket_prompt(root, &prepared);
        let duplicate_key = format!("{}::{}", prepared.ticket_key, prepared.event_key);
        if !duplicates.iter().all(|item| item != &duplicate_key) {
            continue;
        }
        duplicates.push(duplicate_key);
        let suggested_skill =
            tickets::suggested_skill_for_live_ticket_source(root, &prepared).unwrap_or(None);
        let queue_task = channels::create_queue_task_with_metadata(
            root,
            channels::QueueTaskCreateRequest {
                title: format!(
                    "Ticket {} event {}",
                    prepared.ticket_key, prepared.event_type
                ),
                prompt: prompt.clone(),
                thread_key: prepared.thread_key.clone(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: suggested_skill.clone(),
                parent_message_key: None,
                extra_metadata: Some(serde_json::json!({
                    "origin_source_label": format!("ticket:{}", prepared.source_system),
                    "source_system": prepared.source_system.clone(),
                    "ticket_key": prepared.ticket_key.clone(),
                    "ticket_event_key": prepared.event_key.clone(),
                    "ticket_remote_event_id": prepared.remote_event_id.clone(),
                    "ticket_case_id": prepared.case_id.clone(),
                    "ticket_dry_run_id": prepared.dry_run_id.clone(),
                    "ticket_label": prepared.label.clone(),
                    "ticket_bundle_label": prepared.bundle_label.clone(),
                    "ticket_bundle_version": prepared.bundle_version,
                    "ticket_approval_mode": prepared.approval_mode.clone(),
                    "ticket_autonomy_level": prepared.autonomy_level.clone(),
                    "ticket_support_mode": prepared.support_mode.clone(),
                    "ticket_risk_level": prepared.risk_level.clone(),
                    "dedupe_key": format!("ticket-event:{}", prepared.event_key),
                })),
            },
        )?;
        let queue_task =
            channels::lease_queue_task(root, &queue_task.message_key, CHANNEL_ROUTER_LEASE_OWNER)?;
        enqueue_prompt(
            root,
            state,
            QueuedPrompt {
                preview: preview_text(&prompt),
                source_label: format!("ticket:{}", prepared.source_system),
                goal: prepared.summary.clone(),
                prompt,
                suggested_skill,
                leased_message_keys: vec![queue_task.message_key],
                leased_ticket_event_keys: vec![prepared.event_key.clone()],
                thread_key: Some(prepared.thread_key.clone()),
                workspace_root: None,
                ticket_self_work_id: None,
                outbound_email: None,
                outbound_anchor: None,
            },
            format!(
                "Queued ticket {} event {} for dry-run-controlled handling",
                prepared.ticket_key, prepared.event_type
            ),
        );
    }
    if !blocked.is_empty() {
        let _ = tickets::ack_leased_ticket_events(root, &blocked, "blocked");
    }
    Ok(())
}

fn enqueue_prompt(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    prompt: QueuedPrompt,
    event: String,
) {
    let event = decorate_service_event_with_skill(&event, prompt.suggested_skill.as_deref());
    let queued = {
        let mut shared = lock_shared_state(state);
        track_leased_keys_locked(
            &mut shared,
            &prompt.leased_message_keys,
            &prompt.leased_ticket_event_keys,
        );
        let runtime_backoff_remaining = runtime_blocker_backoff_remaining_secs(&shared);
        if let Some(reason) = crate::service::working_hours::hold_reason(root) {
            insert_pending_prompt_ordered(&mut shared.pending_prompts, prompt.clone());
            let pending = shared.pending_prompts.len();
            push_event_locked(
                &mut shared,
                format!("{event} (queue #{pending}, outside working hours: {reason})"),
            );
            true
        } else if shared.busy || runtime_backoff_remaining.is_some() {
            insert_pending_prompt_ordered(&mut shared.pending_prompts, prompt.clone());
            let pending = shared.pending_prompts.len();
            if let Some(remaining_secs) = runtime_backoff_remaining {
                let last_error = shared.last_error.clone().unwrap_or_default();
                let event_key = format!(
                    "runtime-backoff:{}:{}",
                    normalize_token(&clip_text(&last_error, 96)),
                    pending
                );
                if let Err(err) = governance::record_event(
                    root,
                    governance::GovernanceEventRequest {
                        mechanism_id: "runtime_blocker_backoff",
                        conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                        severity: "warning",
                        reason: "hard runtime blocker cooldown is deferring new prompt dispatch",
                        action_taken:
                            "kept the new prompt queued until the runtime cooldown expires",
                        details: serde_json::json!({
                            "remaining_secs": remaining_secs,
                            "pending": pending,
                            "source_label": prompt.source_label,
                            "error": clip_text(&last_error, 180),
                        }),
                        idempotence_key: Some(&event_key),
                    },
                ) {
                    push_event_locked(
                        &mut shared,
                        format!("Runtime blocker backoff event persistence failed: {err}"),
                    );
                }
            }
            ensure_queue_guard_locked(root, &mut shared);
            let pending = shared.pending_prompts.len();
            let reason = runtime_backoff_remaining
                .map(|secs| format!("runtime blocker cooldown {secs}s"))
                .unwrap_or_else(|| "service busy".to_string());
            push_event_locked(&mut shared, format!("{event} (queue #{pending}, {reason})"));
            true
        } else {
            shared.busy = true;
            shared.current_goal_preview = Some(prompt.preview.clone());
            shared.active_source_label = Some(prompt.source_label.clone());
            shared.last_error = None;
            shared.last_reply_chars = None;
            shared.last_progress_epoch_secs = current_epoch_secs();
            push_event_locked(&mut shared, event);
            false
        }
    };
    if !queued {
        start_prompt_worker(root.to_path_buf(), state.clone(), prompt);
    }
}

fn queued_prompt_dispatch_rank(prompt: &QueuedPrompt) -> u8 {
    source_label_dispatch_rank(&prompt.source_label)
}

fn source_label_dispatch_rank(source_label: &str) -> u8 {
    let lowered = source_label.trim().to_ascii_lowercase();
    if lowered == QUEUE_GUARD_SOURCE_LABEL {
        return 5;
    }
    if lowered == "tui"
        || lowered == "email:owner"
        || lowered == "email:founder"
        || lowered == "email:admin"
        || lowered == "meeting:mention"
    {
        return 4;
    }
    if lowered.starts_with("email")
        || lowered.starts_with("jami")
        || lowered.starts_with("teams")
        || lowered.starts_with("meeting")
    {
        return 3;
    }
    if lowered.starts_with("ticket:") {
        return 2;
    }
    1
}

fn insert_pending_prompt_ordered(queue: &mut VecDeque<QueuedPrompt>, prompt: QueuedPrompt) {
    let new_rank = queued_prompt_dispatch_rank(&prompt);
    let guard_offset = usize::from(
        matches!(queue.front(), Some(front) if front.source_label == QUEUE_GUARD_SOURCE_LABEL),
    );
    let insert_at = queue
        .iter()
        .enumerate()
        .skip(guard_offset)
        .find_map(|(idx, existing)| {
            (new_rank > queued_prompt_dispatch_rank(existing)).then_some(idx)
        });
    if let Some(idx) = insert_at {
        queue.insert(idx, prompt);
    } else {
        queue.push_back(prompt);
    }
}

fn inbound_dedupe_key(message: &channels::RoutedInboundMessage) -> String {
    let canonical_text = if !message.body_text.trim().is_empty() {
        message.body_text.as_str()
    } else if !message.preview.trim().is_empty() {
        message.preview.as_str()
    } else {
        message.subject.as_str()
    };
    format!(
        "{}|{}|{}|{}",
        normalize_token(&message.channel),
        normalize_token(&message.thread_key),
        normalize_token(&message.sender_address),
        normalize_token(canonical_text)
    )
}

fn decorate_service_event_with_skill(event: &str, suggested_skill: Option<&str>) -> String {
    let Some(skill) = suggested_skill
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return event.to_string();
    };
    format!("{event} [skill {skill}]")
}

fn maybe_start_next_queued_prompt_locked(
    root: &Path,
    shared: &mut SharedState,
) -> Option<QueuedPrompt> {
    if let Some(reason) = crate::service::working_hours::hold_reason(root) {
        if !shared.pending_prompts.is_empty() {
            push_event_locked(
                shared,
                format!("Deferred queued prompt dispatch outside working hours: {reason}"),
            );
        }
        return None;
    }
    let queued = shared.pending_prompts.pop_front()?;
    shared.busy = true;
    shared.current_goal_preview = Some(queued.preview.clone());
    shared.active_source_label = Some(queued.source_label.clone());
    shared.last_error = None;
    shared.last_reply_chars = None;
    shared.last_progress_epoch_secs = current_epoch_secs();
    push_event_locked(
        shared,
        decorate_service_event_with_skill(
            &format!("Started queued {} prompt", queued.source_label),
            queued.suggested_skill.as_deref(),
        ),
    );
    Some(queued)
}

fn maybe_start_next_queued_prompt_after_recovery_locked(
    root: &Path,
    shared: &mut SharedState,
    outcome_recovery_pending: bool,
) -> Option<QueuedPrompt> {
    if outcome_recovery_pending {
        if !shared.pending_prompts.is_empty() {
            push_event_locked(
                shared,
                "Deferred queued prompt dispatch until outcome-witness recovery is queued"
                    .to_string(),
            );
        }
        return None;
    }
    if let Some(remaining_secs) = runtime_blocker_backoff_remaining_secs(shared) {
        if !shared.pending_prompts.is_empty() {
            push_event_locked(
                shared,
                format!(
                    "Deferred queued prompt dispatch for {}s due to hard runtime blocker",
                    remaining_secs
                ),
            );
        }
        return None;
    }
    maybe_start_next_queued_prompt_locked(root, shared)
}

fn suggested_skill_from_message(message: &channels::RoutedInboundMessage) -> Option<String> {
    if let Some(skill) = message
        .metadata
        .get("skill")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    {
        return Some(skill);
    }
    inferred_skill_from_message_content(message)
}

fn inferred_skill_from_message_content(message: &channels::RoutedInboundMessage) -> Option<String> {
    if !matches!(
        message.channel.as_str(),
        "teams" | "jami" | "whatsapp" | "email" | "tui"
    ) {
        return None;
    }
    if message.channel == "email" {
        let subject = message.subject.to_ascii_lowercase();
        if subject.contains("newsletter") || subject.contains("unsubscribe") {
            return None;
        }
    }
    let text = format!(
        "{}\n{}\n{}",
        message.subject, message.preview, message.body_text
    )
    .to_ascii_lowercase();
    if !looks_like_web_extraction_task(&text) {
        return None;
    }
    Some("universal-scraping".to_string())
}

fn looks_like_web_extraction_task(text: &str) -> bool {
    let has_url = text.contains("http://") || text.contains("https://") || text.contains("www.");
    if !has_url {
        return false;
    }
    let web_source = [
        "webseite",
        "website",
        "seite",
        "portal",
        "ausstellerliste",
        "liste",
        "scroll",
        "lädt erst nach",
        "laedt erst nach",
        "lazy",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    let extraction_intent = [
        "scrap",
        "scrape",
        "auslesen",
        "extrahier",
        "übertrag",
        "uebertrag",
        "excel",
        "xlsx",
        "csv",
        "strukturierte daten",
        "structured data",
        "alle ",
        "massenhaft",
        "liste",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    web_source && extraction_intent
}

fn inbound_source_label(
    settings: &BTreeMap<String, String>,
    message: &channels::RoutedInboundMessage,
) -> String {
    if message.channel == "email" {
        let policy = channels::classify_email_sender(settings, &message.sender_address);
        return match policy.role.as_str() {
            "owner" => "email:owner".to_string(),
            "founder" => "email:founder".to_string(),
            "admin" => "email:admin".to_string(),
            _ => "email".to_string(),
        };
    }
    if message.channel == "meeting"
        && message
            .metadata
            .get("is_mention")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    {
        return "meeting:mention".to_string();
    }
    message.channel.clone()
}

fn meeting_auto_join_policy_block(
    settings: &BTreeMap<String, String>,
    message: &channels::RoutedInboundMessage,
) -> Option<String> {
    let enabled = settings
        .get("CTO_MEETING_AUTO_JOIN_ENABLED")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "true".to_string());
    if matches!(enabled.as_str(), "0" | "false" | "no" | "off") {
        return Some("auto-join disabled by CTO_MEETING_AUTO_JOIN_ENABLED".to_string());
    }
    let allowed = settings
        .get("CTO_MEETING_ALLOWED_INVITE_SENDERS")
        .map(String::as_str)
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if allowed.is_empty() {
        return None;
    }
    let sender = message.sender_address.trim().to_ascii_lowercase();
    let sender_domain = sender.split('@').nth(1).unwrap_or("");
    let matched = allowed.iter().any(|entry| {
        sender == *entry
            || sender_domain == entry.trim_start_matches('@')
            || (entry.starts_with('@') && sender.ends_with(entry))
    });
    (!matched).then(|| "sender is not in CTO_MEETING_ALLOWED_INVITE_SENDERS".to_string())
}

fn isolated_founder_email_thread_key(raw_thread_key: &str, role: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(raw_thread_key.trim().as_bytes());
    let suffix = digest[..8]
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>();
    format!(
        "email-review:{}:{}",
        role.trim().to_ascii_lowercase(),
        suffix
    )
}

fn execution_thread_key_for_inbound_message(
    settings: &BTreeMap<String, String>,
    message: &channels::RoutedInboundMessage,
) -> String {
    if message.channel == "email" {
        let policy = channels::classify_email_sender(settings, &message.sender_address);
        if matches!(policy.role.as_str(), "owner" | "founder" | "admin") {
            return isolated_founder_email_thread_key(&message.thread_key, &policy.role);
        }
    }
    message.thread_key.clone()
}

fn metadata_string(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn ticket_self_work_id_from_metadata(metadata: &Value) -> Option<String> {
    metadata_string(metadata, "ticket_self_work_id")
}

fn founder_outbound_action_from_metadata(
    metadata: &Value,
) -> Option<channels::FounderOutboundAction> {
    metadata
        .get("outbound_email")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn ticket_event_key_from_metadata(metadata: &Value) -> Option<String> {
    metadata_string(metadata, "ticket_event_key")
}

fn ticket_self_work_dedupe_key(item: &tickets::TicketSelfWorkItemView) -> Option<String> {
    metadata_string(&item.metadata, "dedupe_key")
}

fn ticket_self_work_thread_key(item: &tickets::TicketSelfWorkItemView) -> String {
    metadata_string(&item.metadata, "thread_key")
        .unwrap_or_else(|| format!("ticket-self-work:{}", item.work_id))
}

fn ticket_self_work_workspace_root(item: &tickets::TicketSelfWorkItemView) -> Option<String> {
    metadata_string(&item.metadata, "workspace_root")
}

fn ticket_self_work_priority(item: &tickets::TicketSelfWorkItemView) -> String {
    metadata_string(&item.metadata, "priority").unwrap_or_else(|| "high".to_string())
}

fn queue_priority_rank(priority: &str) -> u8 {
    match priority.trim().to_ascii_lowercase().as_str() {
        "urgent" => 3,
        "high" => 2,
        "normal" => 1,
        "low" => 0,
        _ => 1,
    }
}

fn ticket_self_work_parent_message_key(item: &tickets::TicketSelfWorkItemView) -> Option<String> {
    metadata_string(&item.metadata, "parent_message_key")
}

fn ticket_self_work_queue_metadata(item: &tickets::TicketSelfWorkItemView) -> Value {
    let mut metadata = serde_json::json!({});
    let Some(map) = metadata.as_object_mut() else {
        return metadata;
    };
    for key in [
        "inbound_message_key",
        "dedupe_key",
        "origin_source_label",
        "repair_reason",
        "runtime_retry_reason",
        "not_before",
        "outbound_anchor",
    ] {
        if let Some(value) = metadata_string(&item.metadata, key) {
            map.insert(key.to_string(), Value::String(value));
        }
    }
    for key in ["runtime_retry", "outbound_email"] {
        if let Some(value) = item.metadata.get(key) {
            map.insert(key.to_string(), value.clone());
        }
    }
    if item.kind == FOUNDER_COMMUNICATION_REWORK_KIND {
        if let Some(parent_key) = ticket_self_work_parent_message_key(item) {
            map.entry("inbound_message_key".to_string())
                .or_insert_with(|| Value::String(parent_key.clone()));
            map.entry("origin_source_label".to_string())
                .or_insert_with(|| Value::String("email:founder".to_string()));
        }
    }
    metadata
}

fn platform_expertise_resume_prompt(item: &tickets::TicketSelfWorkItemView) -> Option<String> {
    metadata_string(&item.metadata, "resume_prompt")
}

fn platform_expertise_resume_goal(item: &tickets::TicketSelfWorkItemView) -> Option<String> {
    metadata_string(&item.metadata, "resume_goal")
}

fn platform_expertise_resume_preview(item: &tickets::TicketSelfWorkItemView) -> Option<String> {
    metadata_string(&item.metadata, "resume_preview")
}

fn platform_expertise_resume_skill(item: &tickets::TicketSelfWorkItemView) -> Option<String> {
    metadata_string(&item.metadata, "resume_skill")
}

fn platform_expertise_pass_kind(item: &tickets::TicketSelfWorkItemView) -> Option<String> {
    metadata_string(&item.metadata, "pass_kind")
}

fn is_founder_or_owner_email_job(job: &QueuedPrompt) -> bool {
    if job.outbound_email.is_some() {
        return true;
    }
    matches!(
        job.source_label.to_ascii_lowercase().as_str(),
        "email:owner" | "email:founder" | "email:admin"
    )
}

fn is_founder_or_owner_inbound_message(
    settings: &BTreeMap<String, String>,
    message: &channels::RoutedInboundMessage,
) -> bool {
    if message.channel != "email" {
        return false;
    }
    let policy = channels::classify_email_sender(settings, &message.sender_address);
    matches!(policy.role.as_str(), "owner" | "founder" | "admin")
}

fn is_founder_communication_rework_queue_message(message: &channels::RoutedInboundMessage) -> bool {
    if message.channel != "queue" {
        return false;
    }
    let kind = message
        .metadata
        .get("ticket_self_work_kind")
        .and_then(Value::as_str)
        .or_else(|| message.metadata.get("kind").and_then(Value::as_str));
    kind == Some(FOUNDER_COMMUNICATION_REWORK_KIND)
        || message
            .metadata
            .get("inbound_message_key")
            .and_then(Value::as_str)
            .map(|value| value.starts_with("email:"))
            .unwrap_or(false)
}

fn founder_rework_inbound_message_key(message: &channels::RoutedInboundMessage) -> Option<String> {
    message
        .metadata
        .get("inbound_message_key")
        .and_then(Value::as_str)
        .or_else(|| {
            message
                .metadata
                .get("parent_message_key")
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| value.starts_with("email:"))
        .map(ToOwned::to_owned)
}

fn founder_rework_origin_source_label(message: &channels::RoutedInboundMessage) -> Option<String> {
    message
        .metadata
        .get("origin_source_label")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| matches!(*value, "email:owner" | "email:founder" | "email:admin"))
        .map(ToOwned::to_owned)
}

fn render_founder_communication_rework_execution_prompt(
    root: &Path,
    message: &channels::RoutedInboundMessage,
    inbound_message_key: &str,
    raw_rework_body: &str,
) -> String {
    let rework_body = clean_founder_rework_body_for_agent(raw_rework_body);
    let inbound_context = load_founder_inbound_context_for_rework(root, inbound_message_key)
        .unwrap_or_else(|| {
            "Die urspruengliche Founder-/Owner-Mail konnte nicht direkt geladen werden. Rekonstruiere den aktuellen Thread vor der Antwort aus der Kommunikationshistorie.".to_string()
        });
    let title = message.subject.trim();
    let title_line = if title.is_empty() {
        String::new()
    } else {
        format!("Anlass: {title}\n\n")
    };
    format!(
        "{title_line}Du bearbeitest eine blockierte Founder-/Owner-Kommunikation. \
Vor einer Antwort musst du den aktuellen Thread und die fachliche Lage pruefen. \
Wenn ein Ergebnis fehlt, erledige die Nacharbeit zuerst; eine reine Umformulierung reicht nicht.\n\n\
Aktuelle Founder-/Owner-Nachricht:\n\
{inbound_context}\n\n\
Konkrete Nacharbeit:\n\
{rework_body}\n\n\
Ausgabe-Regel:\n\
Schreibe am Ende ausschliesslich den sendefertigen E-Mail-Text fuer den bestehenden Thread. \
Keine internen Statusberichte, keine Arbeitsnotizen, keine Host-Pfade, keine Toolnamen, keine Tabellen- oder Promptbegriffe. \
Der gepruefte Versandpfad entscheidet danach ueber Review und Versand."
    )
}

fn clean_founder_rework_body_for_agent(raw: &str) -> String {
    let mut lines = Vec::new();
    let mut dropped_wrapper = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        let lowered = trimmed.to_ascii_lowercase();
        if lowered.starts_with("bearbeite das veroeffentlichte ctox-self-work")
            || lowered.starts_with("bearbeite das veröffentlichte ctox-self-work")
            || lowered.starts_with("titel:")
            || lowered.starts_with("art:")
            || lowered.starts_with("work-id:")
            || lowered.starts_with("remote-ticket:")
        {
            dropped_wrapper = true;
            continue;
        }
        if dropped_wrapper && trimmed.is_empty() && lines.is_empty() {
            continue;
        }
        let naturalized = trimmed
            .replace("Review summary:", "Kurzfassung:")
            .replace("review summary:", "Kurzfassung:")
            .replace("Review-Kurzfassung:", "Kurzfassung:");
        lines.push(naturalized);
    }
    let cleaned = lines.join("\n").trim().to_string();
    if cleaned.is_empty() {
        "Pruefe die blockierte Antwort, erledige die fehlende fachliche Arbeit, und formuliere danach eine sendefertige Antwort an Founder oder Owner.".to_string()
    } else {
        cleaned
    }
}

fn load_founder_inbound_context_for_rework(
    root: &Path,
    inbound_message_key: &str,
) -> Option<String> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path).ok()?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT sender_address, subject, body_text
            FROM communication_messages
            WHERE message_key = ?1
              AND channel = 'email'
              AND direction = 'inbound'
            LIMIT 1
            "#,
        )
        .ok()?;
    stmt.query_row(params![inbound_message_key], |row| {
        let sender: String = row.get(0)?;
        let subject: String = row.get(1)?;
        let body: String = row.get(2)?;
        Ok(format!(
            "Von: {}\nBetreff: {}\n\n{}",
            sender.trim(),
            subject.trim(),
            body.trim()
        ))
    })
    .ok()
}

fn repair_stalled_founder_communications(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    settings: &BTreeMap<String, String>,
) -> Result<usize> {
    let mut repaired = close_stale_founder_communication_self_work_after_reviewed_reply(root)?;
    let invalid_handled = channels::list_unreviewed_handled_inbound_messages(root, 64)?;
    for message in invalid_handled {
        if !is_founder_or_owner_inbound_message(settings, &message) {
            continue;
        }
        // Bug #1+#2: Auto-submitted inbound (RFC 3834 Auto-Submitted /
        // X-Auto-Response-Suppress) is not actionable founder content.
        // Treat as terminal-handled with a structured NO-SEND verdict
        // so the loop never repromotes it into review_rework.
        if channels::metadata_marks_auto_submitted(&message.metadata) {
            let _ = channels::record_terminal_no_send_verdict(
                root,
                &message.message_key,
                "service-loop",
                "auto-submitted reply (RFC 3834): no founder-action expected",
            );
            continue;
        }
        // Bug #3: respect a previously recorded NO-SEND verdict — do
        // not re-spawn rework for an inbound that has been adjudicated
        // as terminally non-actionable.
        if channels::inbound_message_has_terminal_no_send(root, &message.message_key)
            .unwrap_or(false)
        {
            continue;
        }
        if founder_thread_has_later_reviewed_send(root, &message)? {
            repaired += channels::ack_leased_messages(
                root,
                std::slice::from_ref(&message.message_key),
                "cancelled",
            )
            .unwrap_or(0);
            continue;
        }
        let rework_changed = ensure_founder_communication_rework_runnable(
            root,
            &message,
            "Diese Founder-/Owner-Mail war als erledigt markiert, hat aber keinen exakt geprüften und gesendeten Antwortbeleg.",
        )?;
        let _ = channels::ack_leased_messages(
            root,
            std::slice::from_ref(&message.message_key),
            "review_rework",
        );
        if rework_changed {
            push_event(
                state,
                format!(
                    "Restored unreviewed handled founder communication {} into review rework",
                    message.message_key
                ),
            );
        }
        repaired += 1;
    }
    let candidates = channels::list_stalled_inbound_messages(root, 64)?;
    for message in candidates {
        if !is_founder_or_owner_inbound_message(settings, &message) {
            continue;
        }
        // Bug #1: auto-submitted founder mails (out-of-office, server
        // auto-replies) are not actionable; ack as handled and persist
        // a NO-SEND verdict so future passes don't re-promote them.
        if channels::metadata_marks_auto_submitted(&message.metadata) {
            let _ = channels::record_terminal_no_send_verdict(
                root,
                &message.message_key,
                "service-loop",
                "auto-submitted reply (RFC 3834): no founder-action expected",
            );
            let _ = channels::ack_leased_messages(
                root,
                std::slice::from_ref(&message.message_key),
                "handled",
            );
            repaired += 1;
            continue;
        }
        // Bug #3: structured NO-SEND verdict is sticky.
        if channels::inbound_message_has_terminal_no_send(root, &message.message_key)
            .unwrap_or(false)
        {
            let _ = channels::ack_leased_messages(
                root,
                std::slice::from_ref(&message.message_key),
                "handled",
            );
            continue;
        }
        if channels::founder_reply_sent_after_review_for_message(root, &message.message_key)? {
            repaired += channels::ack_leased_messages(
                root,
                std::slice::from_ref(&message.message_key),
                "handled",
            )
            .unwrap_or(0);
            repaired += close_open_founder_communication_self_work_for_inbound(
                root,
                &message.message_key,
                "Founder communication already has a reviewed sent reply; closing stale rework.",
            )?;
            repaired += cancel_open_founder_communication_rework_queue_for_inbound(
                root,
                &message.message_key,
                "Founder communication already has an exact reviewed sent reply.",
            )?;
            continue;
        }
        if founder_thread_has_later_reviewed_send(root, &message)? {
            repaired += channels::ack_leased_messages(
                root,
                std::slice::from_ref(&message.message_key),
                "cancelled",
            )
            .unwrap_or(0);
            repaired += close_open_founder_communication_self_work_for_inbound(
                root,
                &message.message_key,
                "Founder communication was superseded by a later reviewed send in the same thread.",
            )?;
            repaired += cancel_open_founder_communication_rework_queue_for_inbound(
                root,
                &message.message_key,
                "Superseded by later reviewed founder reply in the same thread.",
            )?;
            continue;
        }
        if founder_thread_has_newer_founder_or_owner_inbound(root, settings, &message)? {
            repaired += channels::ack_leased_messages(
                root,
                std::slice::from_ref(&message.message_key),
                "cancelled",
            )
            .unwrap_or(0);
            repaired += close_open_founder_communication_self_work_for_inbound(
                root,
                &message.message_key,
                "Founder communication was superseded by a newer founder/owner inbound in the same thread.",
            )?;
            repaired += cancel_open_founder_communication_rework_queue_for_inbound(
                root,
                &message.message_key,
                "Superseded by newer founder/owner inbound in the same thread.",
            )?;
            continue;
        }
        let previous_route_status = communication_route_status(root, &message.message_key)?;
        let rework_changed = ensure_founder_communication_rework_runnable(
            root,
            &message,
            "Die Founder-/Owner-Mail blieb ohne geprüften Versand in einem blockierten Routing-Zustand stehen.",
        )?;
        if rework_changed || previous_route_status.as_deref() != Some("review_rework") {
            let _ = channels::ack_leased_messages(
                root,
                std::slice::from_ref(&message.message_key),
                "review_rework",
            );
            push_event(
                state,
                format!(
                    "Restored stalled founder communication {} into review rework",
                    message.message_key
                ),
            );
            repaired += 1;
        }
    }
    Ok(repaired)
}

fn founder_thread_has_later_reviewed_send(
    root: &Path,
    message: &channels::RoutedInboundMessage,
) -> Result<bool> {
    if message.thread_key.trim().is_empty() || message.external_created_at.trim().is_empty() {
        return Ok(false);
    }
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let exists: i64 = conn.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM communication_founder_reply_reviews
        WHERE sent_at IS NOT NULL
          AND sent_at > ?2
          AND COALESCE(json_extract(send_result_json, '$.synthetic'), 0) != 1
          AND COALESCE(json_extract(send_result_json, '$.status'), '') != 'no-send-recorded'
          AND json_extract(action_json, '$.thread_key') = ?1
        LIMIT 1
    )
        "#,
        params![message.thread_key, message.external_created_at],
        |row| row.get(0),
    )?;
    Ok(exists != 0)
}

fn founder_thread_has_newer_founder_or_owner_inbound(
    root: &Path,
    settings: &BTreeMap<String, String>,
    message: &channels::RoutedInboundMessage,
) -> Result<bool> {
    if message.thread_key.trim().is_empty() || message.external_created_at.trim().is_empty() {
        return Ok(false);
    }
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT sender_address
        FROM communication_messages
        WHERE channel = 'email'
          AND direction = 'inbound'
          AND thread_key = ?1
          AND external_created_at > ?2
        ORDER BY external_created_at DESC, observed_at DESC
        LIMIT 16
        "#,
    )?;
    let rows = statement.query_map(
        params![message.thread_key, message.external_created_at],
        |row| row.get::<_, String>(0),
    )?;
    for sender in rows {
        let sender = sender?;
        let policy = channels::classify_email_sender(settings, &sender);
        if matches!(policy.role.as_str(), "owner" | "founder" | "admin") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn cancel_open_founder_communication_rework_queue_for_inbound(
    root: &Path,
    inbound_key: &str,
    reason: &str,
) -> Result<usize> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let now = now_iso_string();
    let updated = conn.execute(
        r#"
        UPDATE communication_routing_state
        SET route_status = 'cancelled',
            lease_owner = NULL,
            leased_at = NULL,
            acked_at = ?3,
            last_error = ?4,
            updated_at = ?3
        WHERE message_key IN (
            SELECT m.message_key
            FROM communication_messages m
            LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
            WHERE m.channel = 'queue'
              AND m.direction = 'inbound'
              AND COALESCE(r.route_status, 'pending') IN (
                    'pending', 'leased', 'blocked', 'failed', 'review_rework'
              )
              AND (
                    json_extract(m.metadata_json, '$.parent_message_key') = ?1
                 OR json_extract(m.metadata_json, '$.inbound_message_key') = ?1
              )
              AND (
                    m.subject LIKE 'Founder communication rework:%'
                 OR m.body_text LIKE '%Founder communication rework%'
                 OR json_extract(m.metadata_json, '$.ticket_self_work_kind') = ?2
              )
        )
        "#,
        params![inbound_key, FOUNDER_COMMUNICATION_REWORK_KIND, now, reason],
    )?;
    Ok(updated)
}

fn close_stale_founder_communication_self_work_after_reviewed_reply(root: &Path) -> Result<usize> {
    let items = tickets::list_ticket_self_work_items(root, Some("local"), None, 512)?;
    let mut closed = 0usize;
    for item in items {
        if item.kind != FOUNDER_COMMUNICATION_REWORK_KIND {
            continue;
        }
        if !matches!(
            item.state.as_str(),
            "open" | "published" | "queued" | "blocked" | "restored"
        ) {
            continue;
        }
        let Some(parent_key) = ticket_self_work_parent_message_key(&item) else {
            continue;
        };
        if communication_route_status(root, &parent_key)?.as_deref() != Some("handled") {
            continue;
        }
        if !channels::founder_reply_sent_after_review_for_message(root, &parent_key)? {
            continue;
        }
        close_ticket_self_work_item(
            root,
            &item.work_id,
            "Founder communication already has a reviewed sent reply; closing stale self-work.",
        );
        closed += 1;
    }
    Ok(closed)
}

fn close_open_founder_communication_self_work_for_inbound(
    root: &Path,
    inbound_message_key: &str,
    note: &str,
) -> Result<usize> {
    let items = tickets::list_ticket_self_work_items(root, Some("local"), None, 512)?;
    let mut closed = 0usize;
    for item in items {
        if item.kind != FOUNDER_COMMUNICATION_REWORK_KIND {
            continue;
        }
        if !matches!(
            item.state.as_str(),
            "open" | "published" | "queued" | "blocked" | "restored"
        ) {
            continue;
        }
        let matches_parent = ticket_self_work_parent_message_key(&item).as_deref()
            == Some(inbound_message_key)
            || metadata_string(&item.metadata, "inbound_message_key").as_deref()
                == Some(inbound_message_key);
        if !matches_parent {
            continue;
        }
        close_ticket_self_work_item(root, &item.work_id, note);
        closed += 1;
    }
    Ok(closed)
}

fn communication_route_status(root: &Path, message_key: &str) -> Result<Option<String>> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    conn.query_row(
        "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
        params![message_key],
        |row| row.get(0),
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn ensure_founder_communication_rework_runnable(
    root: &Path,
    message: &channels::RoutedInboundMessage,
    reason: &str,
) -> Result<bool> {
    // Bug #3: a structured terminal NO-SEND verdict on this inbound is
    // sticky for the lifetime of the inbound message_key. Never spawn
    // a fresh rework that would overwrite the prior NO-SEND review.
    if channels::inbound_message_has_terminal_no_send(root, &message.message_key).unwrap_or(false) {
        return Ok(false);
    }
    // Bug #1: structurally non-actionable inbound (RFC 3834
    // auto-submitted) must not trigger founder-communication rework.
    if channels::metadata_marks_auto_submitted(&message.metadata) {
        return Ok(false);
    }
    if open_founder_communication_rework_for_inbound(root, &message.message_key)? {
        let _ =
            normalize_open_founder_communication_rework_queue_metadata(root, &message.message_key)?;
        return release_stalled_founder_communication_rework_queue_for_inbound(
            root,
            &message.message_key,
        )
        .map(|released| released > 0);
    }
    if let Some(item) = find_founder_communication_rework_self_work(root, &message.message_key)? {
        if matches!(
            item.state.as_str(),
            "closed" | "superseded" | "cancelled" | "handled"
        ) {
            create_founder_communication_repair_rework(root, message, reason)?;
            return Ok(true);
        }
        if item.assigned_to.as_deref() != Some("self") {
            let _ = tickets::assign_ticket_self_work_item(
                root,
                &item.work_id,
                "self",
                "ctox-founder-repair",
                Some("stalled founder communication must be handled before lower-priority work"),
            );
        }
        let queued = requeue_review_rejected_self_work(
            root,
            &item.work_id,
            "Founder communication is stalled without a reviewed sent reply; restore the existing rework and answer the current thread after real rework.",
        )?;
        return Ok(queued.is_some());
    }
    // Bug #4: the inbound-message-key dedupe above does not cover NEW
    // founder mails that arrive on the same thread while a prior rework
    // is `Blocked` by the review-loop circuit-breaker. Each new mail has
    // a fresh `message_key` and would otherwise spawn a new rework on a
    // new `work_id`, bypassing the counter-based circuit-breaker that
    // is keyed on the prior `work_id`.
    //
    // Trigger is purely structural: same isolated thread-key AND prior
    // rework state == "blocked". No string-heuristics on prose content.
    let isolated_thread_key = isolated_founder_email_thread_key(&message.thread_key, "founder");
    if let Some(blocked) =
        find_blocked_founder_communication_rework_self_work_by_thread(root, &isolated_thread_key)?
    {
        eprintln!(
            "ctox governance: founder_rework_blocked_by_thread_circuit thread={} prior_work_id={} new_inbound={}",
            isolated_thread_key, blocked.work_id, message.message_key
        );
        let note = format!(
            "Founder-rework circuit-breaker: a new inbound message on the same thread \
             (message_key={}) arrived while this work is `blocked` after \
             {} review-loop attempts. Suppressed spawning a fresh rework on a new \
             work_id to keep the circuit-breaker effective. Resolve the substantive \
             rework on this thread before answering further inbounds.",
            message.message_key, FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD
        );
        let _ = tickets::append_ticket_self_work_note(
            root,
            &blocked.work_id,
            &note,
            "ctox-founder-repair",
            "internal",
        );
        return Ok(false);
    }
    create_founder_communication_repair_rework(root, message, reason)?;
    Ok(true)
}

fn find_founder_communication_rework_self_work(
    root: &Path,
    inbound_message_key: &str,
) -> Result<Option<tickets::TicketSelfWorkItemView>> {
    let items = tickets::list_ticket_self_work_items(root, Some("local"), None, 512)?;
    Ok(items.into_iter().find(|item| {
        item.kind == FOUNDER_COMMUNICATION_REWORK_KIND
            && metadata_string(&item.metadata, "inbound_message_key").as_deref()
                == Some(inbound_message_key)
    }))
}

/// Bug #4 helper: find a `blocked` founder-communication rework self-work
/// item that lives on the same isolated thread-key as the incoming message.
///
/// This complements `find_founder_communication_rework_self_work`, which is
/// keyed on `inbound_message_key`. When a NEW founder mail arrives on a
/// thread whose previous rework was structurally blocked by the
/// review-loop circuit-breaker (`FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD`
/// reached), the new mail has a different `message_key` and would
/// otherwise escape the circuit-breaker. Trigger is purely structural:
/// same `thread_key` (already isolated by `isolated_founder_email_thread_key`)
/// AND `state == "blocked"`. No prose heuristics.
fn find_blocked_founder_communication_rework_self_work_by_thread(
    root: &Path,
    isolated_thread_key: &str,
) -> Result<Option<tickets::TicketSelfWorkItemView>> {
    let items = tickets::list_ticket_self_work_items(root, Some("local"), Some("blocked"), 512)?;
    Ok(items.into_iter().find(|item| {
        item.kind == FOUNDER_COMMUNICATION_REWORK_KIND
            && metadata_string(&item.metadata, "thread_key").as_deref() == Some(isolated_thread_key)
    }))
}

fn create_founder_communication_repair_rework(
    root: &Path,
    message: &channels::RoutedInboundMessage,
    reason: &str,
) -> Result<channels::QueueTaskView> {
    let title = format!(
        "Founder communication rework: {}",
        clip_text(
            if message.subject.trim().is_empty() {
                &message.message_key
            } else {
                message.subject.trim()
            },
            96,
        )
    );
    let prompt = format!(
        "{reason}\n\n\
Urspruengliche Founder-/Owner-Mail:\n\
Von: {}\n\
Betreff: {}\n\n\
{}\n\n\
Was jetzt zu tun ist:\n\
- Rekonstruiere den aktuellen Thread inklusive Anhängen und letzter Founder-Antworten.\n\
- Prüfe, warum keine geprüfte Antwort gesendet wurde.\n\
- Erledige fehlende fachliche Nacharbeit zuerst; keine reine Umformulierung.\n\
- Antworte im bestehenden Thread konkret auf die aktuelle Nachricht.\n\n\
Ausgabe-Regel: Schreibe am Ende ausschließlich den sendefertigen E-Mail-Text. \
Keine internen Notizen, keine Toolnamen, keine Host-Pfade, keine Prompt- oder Source-Code-Begriffe.",
        display_inbound_sender(message),
        message.subject.trim(),
        if !message.body_text.trim().is_empty() {
            message.body_text.trim()
        } else {
            message.preview.trim()
        },
    );
    create_self_work_backed_queue_task(
        root,
        DurableSelfWorkQueueRequest {
            kind: FOUNDER_COMMUNICATION_REWORK_KIND.to_string(),
            title,
            prompt,
            thread_key: isolated_founder_email_thread_key(&message.thread_key, "founder"),
            workspace_root: message.workspace_root.clone(),
            priority: "urgent".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            parent_message_key: Some(message.message_key.clone()),
            metadata: serde_json::json!({
                "inbound_message_key": message.message_key.clone(),
                "dedupe_key": format!("founder-communication-rework:{}", message.message_key),
                "origin_source_label": "email:founder",
                "repair_reason": reason,
            }),
        },
    )
}

fn open_founder_communication_rework_for_inbound(root: &Path, inbound_key: &str) -> Result<bool> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let exists: i64 = conn.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM communication_messages m
            LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
            WHERE m.channel = 'queue'
              AND m.direction = 'inbound'
              AND COALESCE(r.route_status, 'pending') IN ('pending', 'leased', 'review_rework')
              AND (
                    json_extract(m.metadata_json, '$.parent_message_key') = ?1
                 OR json_extract(m.metadata_json, '$.inbound_message_key') = ?1
              )
              AND (
                    m.subject LIKE 'Founder communication rework:%'
                 OR m.body_text LIKE '%Founder communication rework%'
                 OR json_extract(m.metadata_json, '$.ticket_self_work_kind') = ?2
              )
            LIMIT 1
        )
        "#,
        params![inbound_key, FOUNDER_COMMUNICATION_REWORK_KIND],
        |row| row.get(0),
    )?;
    Ok(exists != 0)
}

fn normalize_open_founder_communication_rework_queue_metadata(
    root: &Path,
    inbound_key: &str,
) -> Result<usize> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT m.message_key, m.metadata_json
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'queue'
          AND m.direction = 'inbound'
          AND COALESCE(r.route_status, 'pending') IN ('pending', 'leased', 'review_rework')
          AND (
                json_extract(m.metadata_json, '$.parent_message_key') = ?1
             OR json_extract(m.metadata_json, '$.inbound_message_key') = ?1
          )
          AND (
                m.subject LIKE 'Founder communication rework:%'
             OR m.body_text LIKE '%Founder communication rework%'
             OR json_extract(m.metadata_json, '$.ticket_self_work_kind') = ?2
          )
        "#,
    )?;
    let rows = statement.query_map(
        params![inbound_key, FOUNDER_COMMUNICATION_REWORK_KIND],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    let tx = conn.unchecked_transaction()?;
    let mut updated = 0usize;
    for (message_key, raw_metadata) in rows {
        let mut metadata =
            serde_json::from_str::<Value>(&raw_metadata).unwrap_or_else(|_| serde_json::json!({}));
        let Some(map) = metadata.as_object_mut() else {
            continue;
        };
        let mut changed = false;
        for (key, value) in [
            ("inbound_message_key", inbound_key.to_string()),
            ("parent_message_key", inbound_key.to_string()),
            ("origin_source_label", "email:founder".to_string()),
            ("priority", "urgent".to_string()),
        ] {
            if map.get(key).and_then(Value::as_str).map(str::trim) != Some(value.as_str()) {
                map.insert(key.to_string(), Value::String(value));
                changed = true;
            }
        }
        if changed {
            tx.execute(
                "UPDATE communication_messages SET metadata_json = ?2 WHERE message_key = ?1",
                params![message_key, serde_json::to_string(&metadata)?],
            )?;
            updated += 1;
        }
    }
    tx.commit()?;
    Ok(updated)
}

fn release_stalled_founder_communication_rework_queue_for_inbound(
    root: &Path,
    inbound_key: &str,
) -> Result<usize> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let now = now_iso_string();
    let updated = conn.execute(
        r#"
        UPDATE communication_routing_state
        SET route_status = 'pending',
            lease_owner = NULL,
            leased_at = NULL,
            acked_at = NULL,
            last_error = 'released stalled founder review-rework queue item',
            updated_at = ?3
        WHERE message_key = (
            SELECT m.message_key
            FROM communication_messages m
            LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
            WHERE m.channel = 'queue'
              AND m.direction = 'inbound'
              AND COALESCE(r.route_status, 'pending') = 'review_rework'
              AND (
                    json_extract(m.metadata_json, '$.parent_message_key') = ?1
                 OR json_extract(m.metadata_json, '$.inbound_message_key') = ?1
              )
              AND (
                    m.subject LIKE 'Founder communication rework:%'
                 OR m.body_text LIKE '%Founder communication rework%'
                 OR json_extract(m.metadata_json, '$.ticket_self_work_kind') = ?2
              )
              AND NOT EXISTS (
                    SELECT 1
                    FROM communication_messages active
                    LEFT JOIN communication_routing_state active_r
                      ON active_r.message_key = active.message_key
                    WHERE active.channel = 'queue'
                      AND active.direction = 'inbound'
                      AND COALESCE(active_r.route_status, 'pending') IN ('pending', 'leased')
                      AND (
                            json_extract(active.metadata_json, '$.parent_message_key') = ?1
                         OR json_extract(active.metadata_json, '$.inbound_message_key') = ?1
                      )
              )
            ORDER BY m.observed_at DESC, m.message_key DESC
            LIMIT 1
        )
        "#,
        params![inbound_key, FOUNDER_COMMUNICATION_REWORK_KIND, now],
    )?;
    Ok(updated)
}

fn founder_email_worker_error_is_retryable(job: &QueuedPrompt, error: &str) -> bool {
    if !is_founder_or_owner_email_job(job) {
        return false;
    }
    let normalized = error.to_ascii_lowercase();
    runtime_error_is_transient_api_failure(error)
        || normalized.contains("database is locked")
        || normalized.contains("database is busy")
        || normalized.contains("sqlite_busy")
        || normalized.contains("sqlite locked")
}

fn runtime_error_is_transient_api_failure(error: &str) -> bool {
    if turn_loop::hard_runtime_blocker_retry_cooldown_secs(error).is_none() {
        return false;
    }
    let normalized = error.to_ascii_lowercase();
    normalized.contains("turn completed without assistant message")
        || normalized.contains("completed without assistant message")
        || normalized.contains("no assistant message")
        || normalized.contains("empty assistant message")
        || normalized.contains("terminal-bench preflight violation")
        || normalized.contains("too many requests")
        || normalized.contains("rate limit")
        || normalized.contains("rate_limit")
        || normalized.contains("http 429")
        || normalized.contains("status 429")
        || normalized.contains("status code 429")
        || normalized.contains("temporarily unavailable")
        || normalized.contains("server overloaded")
        || normalized.contains("bad gateway")
        || normalized.contains("gateway timeout")
        || normalized.contains("service unavailable")
        || normalized.contains("http 502")
        || normalized.contains("http 503")
        || normalized.contains("http 504")
        || normalized.contains("status 502")
        || normalized.contains("status 503")
        || normalized.contains("status 504")
        || normalized.contains("status code 502")
        || normalized.contains("status code 503")
        || normalized.contains("status code 504")
}

fn founder_email_reply_message_key(job: &QueuedPrompt) -> Option<&str> {
    if !is_founder_or_owner_email_job(job) {
        return None;
    }
    job.leased_message_keys
        .iter()
        .find(|key| key.starts_with("email:"))
        .map(|key| key.as_str())
}

fn founder_outbound_anchor_key(job: &QueuedPrompt) -> Option<&str> {
    // Prefer an explicit operator-set anchor (e.g. TUI-initiated proactive
    // outbound where there is no leased inbound message). Fall back to the
    // first leased message key for inbound-driven jobs. Never derived from
    // prompt text — this is structural.
    if let Some(anchor) = job.outbound_anchor.as_deref() {
        return Some(anchor);
    }
    job.leased_message_keys.first().map(|key| key.as_str())
}

fn detect_founder_mail_commitments(text: &str) -> Vec<String> {
    let normalized = text.replace('\n', " ");
    normalized
        .split_terminator(['.', '!', '?'])
        .filter_map(|segment| {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                return None;
            }
            let lowered = trimmed.to_ascii_lowercase();
            let has_commitment_verb = [
                "i will", "i'll", "we will", "we'll", "send", "deliver", "provide", "share",
                "update", "inform", "sende", "liefere", "schicke", "melde",
            ]
            .iter()
            .any(|needle| lowered.contains(needle));
            if !has_commitment_verb {
                return None;
            }
            let has_future_marker = lowered.contains("today")
                || lowered.contains("tomorrow")
                || lowered.contains("heute")
                || lowered.contains("morgen")
                || lowered.contains(" by ")
                || lowered.contains(" bis ")
                || lowered.contains(" until ")
                || lowered.contains("utc")
                || contains_clock_time(&lowered)
                || contains_calendar_date(&lowered);
            if !has_future_marker {
                return None;
            }
            Some(trimmed.to_string())
        })
        .collect()
}

fn contains_clock_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(5).any(|window| {
        window[0].is_ascii_digit()
            && window[1].is_ascii_digit()
            && window[2] == b':'
            && window[3].is_ascii_digit()
            && window[4].is_ascii_digit()
    })
}

fn contains_calendar_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(10).any(|window| {
        window[0].is_ascii_digit()
            && window[1].is_ascii_digit()
            && (window[2] == b'.' || window[2] == b'/')
            && window[3].is_ascii_digit()
            && window[4].is_ascii_digit()
            && (window[5] == b'.' || window[5] == b'/')
            && window[6].is_ascii_digit()
            && window[7].is_ascii_digit()
            && window[8].is_ascii_digit()
            && window[9].is_ascii_digit()
    })
}

fn founder_commitment_backing_summaries(root: &Path) -> Vec<String> {
    schedule::list_tasks(root)
        .unwrap_or_default()
        .into_iter()
        .filter(|task| task.enabled)
        .map(|task| {
            format!(
                "{} @ {}",
                task.name,
                task.next_run_at
                    .unwrap_or_else(|| "(no next run)".to_string())
            )
        })
        .collect()
}

fn founder_commitment_guard_outcome(
    commitments: &[String],
    backing: &[String],
) -> Option<review::ReviewOutcome> {
    if commitments.is_empty() || backing.len() >= commitments.len() {
        return None;
    }
    Some(review::ReviewOutcome {
        required: true,
        verdict: review::ReviewVerdict::Fail,
        mission_state: "UNHEALTHY".to_string(),
        summary: format!(
            "Founder mail makes {} future commitment(s) but only {} tracked schedule/follow-up backing item(s) exist.",
            commitments.len(),
            backing.len()
        ),
        report: String::new(),
        score: 100,
        reasons: vec!["unbacked_commitment".to_string()],
        failed_gates: vec!["unbacked_commitment".to_string()],
        semantic_findings: commitments
            .iter()
            .map(|item| format!("Commitment requires backing before send: {item}"))
            .collect(),
        categorized_findings: Vec::new(),
        open_items: vec![
            "Create concrete CTOX schedule or follow-up backing for every promised founder deadline before sending."
                .to_string(),
        ],
        evidence: if backing.is_empty() {
            vec!["No enabled CTOX schedule backing was found.".to_string()]
        } else {
            backing
                .iter()
                .map(|item| format!("Available backing: {item}"))
                .collect()
        },
        handoff: None,
        disposition: review::ReviewDisposition::Send,
    })
}

fn is_owner_visible_strategic_job(job: &QueuedPrompt) -> bool {
    if !derive_owner_visible_for_review(&job.source_label) {
        return false;
    }
    let source = job.source_label.to_ascii_lowercase();
    if source == "review-feedback" || source == "outcome-witness-recovery" {
        return false;
    }
    if is_founder_or_owner_email_job(job) {
        return false;
    }
    if is_internal_harness_or_forensics_job(job) {
        return false;
    }
    if is_bounded_stateful_product_execution_job(job) {
        return false;
    }
    if is_bounded_benchmark_or_runtime_execution_job(job) {
        return false;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}\n{}",
        job.prompt,
        job.goal,
        job.preview,
        job.thread_key.clone().unwrap_or_default(),
        job.workspace_root.clone().unwrap_or_default()
    )
    .to_ascii_lowercase();
    haystack.contains("homepage")
        || haystack.contains("landing")
        || haystack.contains("website")
        || haystack.contains("product")
        || haystack.contains("platform")
        || haystack.contains("marketplace")
        || haystack.contains("public")
        || haystack.contains("founder")
        || haystack.contains("buyer")
        || haystack.contains("customer")
}

fn is_bounded_stateful_product_execution_job(job: &QueuedPrompt) -> bool {
    job.workspace_root.is_some()
        && job.suggested_skill.as_deref() == Some("stateful-product-from-scratch")
        && !job.leased_message_keys.iter().any(|key| {
            key.starts_with("email:") || key.starts_with("jami:") || key.starts_with("meeting:")
        })
}

fn is_bounded_benchmark_or_runtime_execution_job(job: &QueuedPrompt) -> bool {
    let haystack = format!(
        "{}\n{}\n{}\n{}\n{}",
        job.prompt,
        job.goal,
        job.preview,
        job.thread_key.clone().unwrap_or_default(),
        job.workspace_root.clone().unwrap_or_default()
    )
    .to_ascii_lowercase();
    let terminal_bench_scope = haystack.contains("terminal-bench")
        || haystack.contains("terminal bench")
        || haystack.contains("terminal_bench")
        || haystack.contains("terminalbench")
        || haystack.contains("tbench")
        || haystack.contains("tb2");
    let benchmark_scope = terminal_bench_scope
        || (haystack.contains("benchmark")
            && (haystack.contains("runner")
                || haystack.contains("results.jsonl")
                || haystack.contains("run-log.md")));
    let terminal_bench_prep_or_control = terminal_bench_scope
        && (haystack.contains("prep-")
            || haystack.contains("preparation ticket")
            || haystack.contains("preparation_tickets")
            || haystack.contains("task inventory")
            || haystack.contains("reference research")
            || haystack.contains("leaderboard")
            || haystack.contains("priority plan")
            || haystack.contains("smoke")
            || haystack.contains("controller")
            || haystack.contains("ticket-map.jsonl")
            || haystack.contains("run-queue.jsonl")
            || haystack.contains("results.jsonl")
            || haystack.contains("knowledge.md")
            || haystack.contains("logbook.md")
            || haystack.contains("harbor")
            || haystack.contains("no solution"));
    let concrete_execution = haystack.contains("run_dir=")
        || haystack.contains("required output artifacts")
        || haystack.contains("required artifacts")
        || haystack.contains("required files:")
        || haystack.contains("required files")
        || haystack.contains("durable artifact contract")
        || haystack.contains("required durable files")
        || haystack.contains("write these exact files")
        || haystack.contains("use shell/tools")
        || haystack.contains("use shell tools")
        || haystack.contains("harbor")
        || haystack.contains("local ipc")
        || haystack.contains("local ctox ipc")
        || haystack.contains("context_window")
        || haystack.contains("context window")
        || haystack.contains("backend evidence")
        || haystack.contains("verify runtime/context");
    terminal_bench_prep_or_control || (benchmark_scope && concrete_execution)
}

fn is_internal_harness_or_forensics_job(job: &QueuedPrompt) -> bool {
    let thread_key = job
        .thread_key
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if thread_key.starts_with("codex/")
        || thread_key.starts_with("internal/")
        || thread_key.contains("harness")
        || thread_key.contains("process-mining")
    {
        return true;
    }
    let haystack = format!("{}\n{}\n{}", job.prompt, job.goal, job.preview).to_ascii_lowercase();
    (haystack.contains("harness-smoke") || haystack.contains("process-mining"))
        && (haystack.contains("keine externe kommunikation")
            || haystack.contains("no external communication"))
}

fn queue_strategy_direction_pass(
    root: &Path,
    thread_key: &str,
    workspace_root: Option<&str>,
    resume_prompt: &str,
    resume_goal: &str,
    resume_preview: &str,
    resume_skill: Option<&str>,
) -> Result<channels::QueueTaskView> {
    let conversation_id = turn_loop::conversation_id_for_thread_key(Some(thread_key));
    let deferred_target = compact_deferred_target(resume_prompt);
    let deferred_goal = compact_deferred_metadata(resume_goal);
    let deferred_preview = compact_deferred_metadata(resume_preview);
    create_self_work_backed_queue_task(
        root,
        DurableSelfWorkQueueRequest {
            kind: STRATEGIC_DIRECTION_KIND.to_string(),
            title: "Strategic direction setup".to_string(),
            prompt: format!(
                "Before further strategic or owner-visible execution, establish canonical runtime direction in SQLite.\n\n\
Required outputs:\n\
- create or revise an active Vision record in SQLite-backed runtime state\n\
- create or revise an active Mission record in SQLite-backed runtime state\n\
- if founder or CEO guidance changed the direction, persist that revision with the decision reason\n\
- do not treat markdown files or chat text as canonical knowledge\n\
\n\
Required strategy scope for every strategy command in this slice:\n\
- `--conversation-id {}`\n\
- `--thread-key {}`\n\
- Never write global directives without these scope flags.\n\
\n\
Use `ctox strategy show --conversation-id {} --thread-key {}` first.\n\
When creating or revising canonical direction, use `ctox strategy set --conversation-id {} --thread-key {}` or `ctox strategy propose --conversation-id {} --thread-key {}`.\n\
The authoritative Vision and Mission must live in runtime SQLite state before implementation continues.\n\
\n\
After direction is canonical, the deferred execution target is:\n{}",
                conversation_id,
                thread_key,
                conversation_id,
                thread_key,
                conversation_id,
                thread_key,
                conversation_id,
                thread_key,
                deferred_target
            ),
            thread_key: thread_key.to_string(),
            workspace_root: workspace_root.map(ToOwned::to_owned),
            priority: "urgent".to_string(),
            suggested_skill: Some(
                resume_skill
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("plan-orchestrator")
                    .to_string(),
            ),
            parent_message_key: None,
            metadata: serde_json::json!({
                "thread_key": thread_key,
                "workspace_root": workspace_root,
                "priority": "urgent",
                "skill": resume_skill,
                "resume_prompt": deferred_target,
                "resume_goal": deferred_goal,
                "resume_preview": deferred_preview,
                "resume_skill": resume_skill,
                "dedupe_key": format!("strategy-direction:{}", thread_key),
            }),
        },
    )
}

fn cancel_runnable_thread_tasks_for_strategy(
    root: &Path,
    thread_key: &str,
    except_message_keys: &[String],
) -> Result<usize> {
    let tasks =
        channels::list_queue_tasks(root, &["pending".to_string(), "leased".to_string()], 128)?;
    let note = "Cancelled because canonical Vision and Mission must be established in SQLite before strategic work on this thread can continue.";
    let mut cancelled = 0usize;
    for task in tasks.into_iter().filter(|task| {
        task.thread_key == thread_key
            && !except_message_keys
                .iter()
                .any(|key| key == &task.message_key)
    }) {
        channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: task.message_key.clone(),
                route_status: Some("cancelled".to_string()),
                status_note: Some(note.to_string()),
                ..Default::default()
            },
        )?;
        if let Some(work_id) = task.ticket_self_work_id.as_deref() {
            supersede_ticket_self_work_item(root, work_id, note);
        }
        cancelled += 1;
    }
    Ok(cancelled)
}

fn has_runnable_founder_or_owner_email(root: &Path) -> Result<bool> {
    let settings = live_service_settings(root);
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT m.sender_address
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'email'
          AND m.direction = 'inbound'
          AND COALESCE(r.route_status, 'pending') IN ('pending', 'leased')
        "#,
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    for sender in rows {
        let sender = sender?;
        let role = channels::classify_email_sender(&settings, &sender).role;
        if matches!(role.as_str(), "owner" | "founder" | "admin") {
            return Ok(true);
        }
    }
    Ok(false)
}

const DEFERRED_TARGET_MAX_CHARS: usize = 4_000;
const DEFERRED_METADATA_MAX_CHARS: usize = 4_000;

fn compact_deferred_target(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "reconstruct the deferred target from durable queue, ticket, and continuity state"
            .to_string();
    }
    clip_text(trimmed, DEFERRED_TARGET_MAX_CHARS)
}

fn compact_deferred_metadata(value: &str) -> String {
    clip_text(value.trim(), DEFERRED_METADATA_MAX_CHARS)
}

fn maybe_redirect_owner_visible_work_to_strategy_setup(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    job: &QueuedPrompt,
) -> Result<bool> {
    let current_item = job.ticket_self_work_id.as_deref().and_then(|work_id| {
        tickets::load_ticket_self_work_item(root, work_id)
            .ok()
            .flatten()
    });
    if matches!(
        current_item.as_ref().map(|item| item.kind.as_str()),
        Some("timeout-continuation" | "runtime-api-retry")
    ) {
        return Ok(false);
    }
    if job.outbound_email.is_some() || job.outbound_anchor.is_some() {
        return Ok(false);
    }
    if !is_owner_visible_strategic_job(job) {
        return Ok(false);
    }
    if has_runnable_founder_or_owner_email(root)? {
        return Ok(false);
    }
    if current_item.as_ref().map(|item| item.kind.as_str()) == Some(STRATEGIC_DIRECTION_KIND) {
        return Ok(false);
    }
    let thread_key = job
        .thread_key
        .clone()
        .unwrap_or_else(|| default_follow_up_thread_key(&job.goal));
    let conversation_id = turn_loop::conversation_id_for_thread_key(Some(thread_key.as_str()));
    let db_path = crate::paths::core_db(&root);
    let engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;
    let strategy = engine.active_strategy_snapshot(conversation_id, Some(thread_key.as_str()))?;
    if strategy.active_vision.is_some() && strategy.active_mission.is_some() {
        return Ok(false);
    }
    let cancelled_thread_tasks =
        cancel_runnable_thread_tasks_for_strategy(root, &thread_key, &job.leased_message_keys)?;
    if !job.leased_message_keys.is_empty() {
        let _ = channels::ack_leased_messages(root, &job.leased_message_keys, "cancelled");
    }
    if !job.leased_ticket_event_keys.is_empty() {
        let _ = tickets::ack_leased_ticket_events(root, &job.leased_ticket_event_keys, "blocked");
    }
    if let Some(work_id) = job.ticket_self_work_id.as_deref() {
        supersede_ticket_self_work_item(
            root,
            work_id,
            "Closed without execution because canonical Vision and Mission must be established in SQLite before strategic work continues.",
        );
    }
    let created = queue_strategy_direction_pass(
        root,
        &thread_key,
        job.workspace_root.as_deref(),
        &job.prompt,
        &job.goal,
        &job.preview,
        job.suggested_skill.as_deref(),
    )?;
    let mut next_prompt = None;
    {
        let mut shared = lock_shared_state(state);
        shared.busy = false;
        shared.current_goal_preview = None;
        shared.active_source_label = None;
        shared.last_completed_at = Some(now_iso_string());
        shared.last_progress_epoch_secs = current_epoch_secs();
        shared.last_reply_chars = None;
        shared.last_error = None;
        release_leased_keys_locked(
            &mut shared,
            &job.leased_message_keys,
            &job.leased_ticket_event_keys,
        );
        push_event_locked(
            &mut shared,
            format!(
                "Rerouted strategic work to canonical direction setup: {} (cancelled {} competing runnable task(s) on the thread)",
                created.title, cancelled_thread_tasks
            ),
        );
        if runtime_blocker_backoff_remaining_secs(&shared).is_none() {
            next_prompt = maybe_start_next_queued_prompt_locked(root, &mut shared);
        }
    }
    if let Some(queued) = next_prompt {
        start_prompt_worker(root.to_path_buf(), state.clone(), queued);
    }
    Ok(true)
}

fn is_owner_visible_platform_reset_job(job: &QueuedPrompt) -> bool {
    if !derive_owner_visible_for_review(&job.source_label) {
        return false;
    }
    if is_founder_or_owner_email_job(job) {
        return false;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}\n{}",
        job.prompt,
        job.goal,
        job.preview,
        job.thread_key.clone().unwrap_or_default(),
        job.workspace_root.clone().unwrap_or_default()
    )
    .to_ascii_lowercase();
    let kunstmen_scope = haystack.contains("kunstmen")
        || job
            .workspace_root
            .as_deref()
            .map(|path| path.contains("/kunstmen"))
            .unwrap_or(false);
    if !kunstmen_scope {
        return false;
    }
    haystack.contains("homepage")
        || haystack.contains("landing")
        || haystack.contains("platform")
        || haystack.contains("marketplace")
        || haystack.contains("catalog")
        || haystack.contains("roster")
        || haystack.contains("hire")
}

fn list_platform_expertise_scope_items(
    root: &Path,
    thread_key: &str,
    workspace_root: Option<&str>,
) -> Result<Vec<tickets::TicketSelfWorkItemView>> {
    let items = tickets::list_ticket_self_work_items(root, Some("local"), None, 512)?;
    Ok(items
        .into_iter()
        .filter(|item| {
            matches!(
                item.kind.as_str(),
                PLATFORM_EXPERTISE_KIND | PLATFORM_IMPLEMENTATION_KIND
            )
        })
        .filter(|item| {
            let item_thread_key = ticket_self_work_thread_key(item);
            if item_thread_key == thread_key {
                return true;
            }
            if let (Some(lhs), Some(rhs)) = (
                workspace_root
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                ticket_self_work_workspace_root(item)
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
            ) {
                return lhs == rhs;
            }
            false
        })
        .collect())
}

fn next_missing_platform_expertise_pass(
    items: &[tickets::TicketSelfWorkItemView],
) -> Option<ExpertisePassSpec> {
    for spec in PLATFORM_EXPERTISE_PASSES {
        let mut completed = false;
        let mut in_progress = false;
        for item in items {
            if item.kind != PLATFORM_EXPERTISE_KIND {
                continue;
            }
            if platform_expertise_pass_kind(item).as_deref() != Some(spec.pass_kind) {
                continue;
            }
            match item.state.as_str() {
                "closed" => completed = true,
                "open" | "queued" | "published" | "blocked" => in_progress = true,
                _ => {}
            }
        }
        if completed {
            continue;
        }
        if in_progress {
            return Some(spec);
        }
        return Some(spec);
    }
    None
}

fn queue_platform_expertise_pass(
    root: &Path,
    thread_key: &str,
    workspace_root: Option<&str>,
    spec: ExpertisePassSpec,
    resume_prompt: &str,
    resume_goal: &str,
    resume_preview: &str,
    resume_skill: Option<&str>,
) -> Result<channels::QueueTaskView> {
    let conversation_id = turn_loop::conversation_id_for_thread_key(Some(thread_key));
    let deferred_target = compact_deferred_target(resume_prompt);
    let deferred_goal = compact_deferred_metadata(resume_goal);
    let deferred_preview = compact_deferred_metadata(resume_preview);
    create_self_work_backed_queue_task(
        root,
        DurableSelfWorkQueueRequest {
            kind: PLATFORM_EXPERTISE_KIND.to_string(),
            title: format!("Kunstmen {} pass", spec.display_name),
            prompt: format!(
                "This slice is the dedicated {} pass for the current public Kunstmen platform mission.\n\n\
Required outputs:\n\
- stay inside this discipline only\n\
- persist the result into canonical CTOX runtime state via `ctox` CLI commands\n\
- do not write durable claims into `<workspace>/runtime/ctox.sqlite3`\n\
- if this pass produces a durable finding or design deliverable, record it with `ctox verification claim-set --conversation-id {} --kind design_artifact --status verified --subject <subject> --summary <summary> --evidence <evidence>`\n\
- a markdown file in the workspace does not count as durable state; reusable procedure must be in source-skill, Skillbook, Runbook, or Runbook-Item records\n\
- leave the implementation pass with concrete, structured guidance for what to build next\n\
\n\
Discipline to resolve now: {}\n\
Future implementation target after all passes complete:\n{}",
                spec.display_name, conversation_id, spec.display_name, deferred_target
            ),
            thread_key: thread_key.to_string(),
            workspace_root: workspace_root.map(ToOwned::to_owned),
            priority: "urgent".to_string(),
            suggested_skill: Some(spec.suggested_skill.to_string()),
            parent_message_key: None,
            metadata: serde_json::json!({
                "thread_key": thread_key,
                "workspace_root": workspace_root,
                "priority": "urgent",
                "skill": spec.suggested_skill,
                "pass_kind": spec.pass_kind,
                "resume_prompt": deferred_target,
                "resume_goal": deferred_goal,
                "resume_preview": deferred_preview,
                "resume_skill": resume_skill,
                "dedupe_key": format!("platform-pass:{}:{}", thread_key, spec.pass_kind),
            }),
        },
    )
}

fn queue_platform_implementation_resume(
    root: &Path,
    thread_key: &str,
    workspace_root: Option<&str>,
    resume_prompt: &str,
    resume_goal: &str,
    resume_preview: &str,
    resume_skill: Option<&str>,
) -> Result<Option<channels::QueueTaskView>> {
    let conversation_id = turn_loop::conversation_id_for_thread_key(Some(thread_key));
    let deferred_target = compact_deferred_target(resume_prompt);
    let deferred_goal = compact_deferred_metadata(resume_goal);
    let deferred_preview = compact_deferred_metadata(resume_preview);
    let items = list_platform_expertise_scope_items(root, thread_key, workspace_root)?;
    if items.iter().any(|item| {
        item.kind == PLATFORM_IMPLEMENTATION_KIND
            && matches!(
                item.state.as_str(),
                "open" | "queued" | "published" | "blocked"
            )
    }) {
        return Ok(None);
    }
    create_self_work_backed_queue_task(
        root,
        DurableSelfWorkQueueRequest {
            kind: PLATFORM_IMPLEMENTATION_KIND.to_string(),
            title: "Kunstmen platform implementation reset".to_string(),
            prompt: format!(
                "All required pre-implementation CTO passes for this public Kunstmen platform work are complete in SQLite-backed runtime state.\n\n\
Execute the implementation slice now.\n\
Build a platform front door, not a poster.\n\
The public buyer path must make these steps obvious:\n\
- choose an AI employee / expert from a roster\n\
- inspect a concrete profile\n\
- start interview / application chat\n\
- hire / checkout\n\
\n\
No prompt leakage, no source-code leakage, no operator/admin language.\n\
Persist any completion claim or durable design artifact into the canonical CTOX runtime DB with `ctox verification claim-set --conversation-id {} --kind design_artifact --status verified --subject <subject> --summary <summary> --evidence <evidence>`.\n\
Do not treat `<workspace>/runtime/ctox.sqlite3` as canonical state.\n\n\
Implementation objective:\n{}",
                conversation_id, deferred_target
            ),
            thread_key: thread_key.to_string(),
            workspace_root: workspace_root.map(ToOwned::to_owned),
            priority: "urgent".to_string(),
            suggested_skill: Some(
                resume_skill
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("frontend-skill")
                    .to_string(),
            ),
            parent_message_key: None,
            metadata: serde_json::json!({
                "thread_key": thread_key,
                "workspace_root": workspace_root,
                "priority": "urgent",
                "skill": resume_skill,
                "resume_prompt": deferred_target,
                "resume_goal": deferred_goal,
                "resume_preview": deferred_preview,
                "resume_skill": resume_skill,
                "dedupe_key": format!("platform-implementation:{}", thread_key),
            }),
        },
    )
    .map(Some)
}

fn maybe_continue_platform_expertise_pipeline_after_success(
    root: &Path,
    job: &QueuedPrompt,
) -> Result<Option<String>> {
    let Some(work_id) = job.ticket_self_work_id.as_deref() else {
        return Ok(None);
    };
    let Some(item) = tickets::load_ticket_self_work_item(root, work_id)? else {
        return Ok(None);
    };
    if item.kind != PLATFORM_EXPERTISE_KIND {
        return Ok(None);
    }
    let thread_key = ticket_self_work_thread_key(&item);
    let workspace_root = ticket_self_work_workspace_root(&item);
    let resume_prompt = platform_expertise_resume_prompt(&item)
        .unwrap_or_else(|| fallback_text(&job.prompt, &job.goal).to_string());
    let resume_goal = platform_expertise_resume_goal(&item)
        .unwrap_or_else(|| fallback_text(&job.goal, &job.preview).to_string());
    let resume_preview = platform_expertise_resume_preview(&item)
        .unwrap_or_else(|| fallback_text(&job.preview, &job.goal).to_string());
    let resume_skill = platform_expertise_resume_skill(&item);
    let items = list_platform_expertise_scope_items(root, &thread_key, workspace_root.as_deref())?;
    if let Some(next_pass) = next_missing_platform_expertise_pass(&items) {
        let created = queue_platform_expertise_pass(
            root,
            &thread_key,
            workspace_root.as_deref(),
            next_pass,
            &resume_prompt,
            &resume_goal,
            &resume_preview,
            resume_skill.as_deref(),
        )?;
        return Ok(Some(format!(
            "Queued next required CTO pass: {} ({})",
            created.title, next_pass.pass_kind
        )));
    }
    let created = queue_platform_implementation_resume(
        root,
        &thread_key,
        workspace_root.as_deref(),
        &resume_prompt,
        &resume_goal,
        &resume_preview,
        resume_skill.as_deref(),
    )?;
    Ok(created.map(|task| format!("Queued platform implementation resume: {}", task.title)))
}

fn maybe_redirect_platform_work_to_expertise_passes(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    job: &QueuedPrompt,
) -> Result<bool> {
    if !is_owner_visible_platform_reset_job(job) {
        return Ok(false);
    }
    let current_item = job.ticket_self_work_id.as_deref().and_then(|work_id| {
        tickets::load_ticket_self_work_item(root, work_id)
            .ok()
            .flatten()
    });
    if matches!(
        current_item.as_ref().map(|item| item.kind.as_str()),
        Some(PLATFORM_EXPERTISE_KIND | PLATFORM_IMPLEMENTATION_KIND | STRATEGIC_DIRECTION_KIND)
    ) {
        return Ok(false);
    }
    let thread_key = job
        .thread_key
        .clone()
        .unwrap_or_else(|| default_follow_up_thread_key(&job.goal));
    let workspace_root = job.workspace_root.clone();
    let items = list_platform_expertise_scope_items(root, &thread_key, workspace_root.as_deref())?;
    let Some(next_pass) = next_missing_platform_expertise_pass(&items) else {
        return Ok(false);
    };
    if !job.leased_message_keys.is_empty() {
        let _ = channels::ack_leased_messages(root, &job.leased_message_keys, "cancelled");
    }
    if !job.leased_ticket_event_keys.is_empty() {
        let _ = tickets::ack_leased_ticket_events(root, &job.leased_ticket_event_keys, "blocked");
    }
    let created = queue_platform_expertise_pass(
        root,
        &thread_key,
        workspace_root.as_deref(),
        next_pass,
        &job.prompt,
        &job.goal,
        &job.preview,
        job.suggested_skill.as_deref(),
    )?;
    if let Some(work_id) = job.ticket_self_work_id.as_deref() {
        supersede_ticket_self_work_item(
            root,
            work_id,
            &format!(
                "Closed without execution because owner-visible platform work must first pass through the `{}` CTO discipline slice.",
                next_pass.pass_kind
            ),
        );
    }

    let mut next_prompt = None;
    {
        let mut shared = lock_shared_state(state);
        shared.busy = false;
        shared.current_goal_preview = None;
        shared.active_source_label = None;
        shared.last_completed_at = Some(now_iso_string());
        shared.last_progress_epoch_secs = current_epoch_secs();
        shared.last_reply_chars = None;
        shared.last_error = None;
        release_leased_keys_locked(
            &mut shared,
            &job.leased_message_keys,
            &job.leased_ticket_event_keys,
        );
        push_event_locked(
            &mut shared,
            format!(
                "Redirected owner-visible platform work into required CTO pass `{}` via {}",
                next_pass.pass_kind, created.title
            ),
        );
        if runtime_blocker_backoff_remaining_secs(&shared).is_none() {
            next_prompt = maybe_start_next_queued_prompt_locked(root, &mut shared);
        }
    }
    if let Some(queued) = next_prompt {
        start_prompt_worker(root.to_path_buf(), state.clone(), queued);
    }
    Ok(true)
}

fn resolve_review_rejection_target_self_work_id(root: &Path, job: &QueuedPrompt) -> Option<String> {
    let current_work_id = job.ticket_self_work_id.as_deref()?;
    let item = tickets::load_ticket_self_work_item(root, current_work_id)
        .ok()
        .flatten()?;
    if item.kind != "review-rework" {
        return Some(current_work_id.to_string());
    }
    let parent_key = ticket_self_work_parent_message_key(&item)?;
    let parent_task = channels::load_queue_task(root, &parent_key)
        .ok()
        .flatten()?;
    parent_task
        .ticket_self_work_id
        .filter(|work_id| work_id != current_work_id)
        .or_else(|| Some(current_work_id.to_string()))
}

fn merge_metadata_value(target: &mut Value, extra: Value) {
    let Some(target_map) = target.as_object_mut() else {
        return;
    };
    let Some(extra_map) = extra.as_object() else {
        return;
    };
    for (key, value) in extra_map {
        target_map.insert(key.clone(), value.clone());
    }
}

fn render_ticket_self_work_prompt(root: &Path, item: &tickets::TicketSelfWorkItemView) -> String {
    let mut prompt_lines = vec![
        "SELF-WORK TASK".to_string(),
        format!("- Source system: {}", item.source_system),
        format!("- Title: {}", item.title.trim()),
        format!("- Work id: {}", item.work_id.trim()),
        format!("- Work type: {}", item.kind.trim()),
        String::new(),
        "CONTRACT".to_string(),
        "- Work on this parent task, not on a new unrelated task.".to_string(),
        "- If review notes are listed below, fix the underlying issue they describe.".to_string(),
        "- If the work is not finished by the end of the turn, persist exactly one next runtime item.".to_string(),
        String::new(),
        item.body_text.trim().to_string(),
    ];
    if let Ok(notes) = recent_ticket_self_work_notes_for_prompt(root, &item.work_id, 6) {
        if !notes.is_empty() {
            prompt_lines.push(String::new());
            prompt_lines.push(
                "Aktuelle Rework- und Review-Hinweise, die du zwingend beruecksichtigen musst:"
                    .to_string(),
            );
            for note in notes {
                prompt_lines.push(format!("- {note}"));
            }
        }
    }
    if let Some(locator) = item
        .remote_locator
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt_lines.push(String::new());
        prompt_lines.push(format!("Remote-Ticket: {}", locator));
    }
    prompt_lines.join("\n")
}

fn recent_ticket_self_work_notes_for_prompt(
    root: &Path,
    work_id: &str,
    limit: usize,
) -> Result<Vec<String>> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT body_text
        FROM ticket_self_work_notes
        WHERE work_id = ?1
          AND TRIM(body_text) <> ''
        ORDER BY created_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![work_id, limit as i64], |row| {
        row.get::<_, String>(0)
    })?;
    let mut notes = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    notes.reverse();
    let mut rendered = Vec::new();
    for note in notes {
        let trimmed = note.trim();
        if trimmed.is_empty() || is_internal_routing_note(trimmed) {
            continue;
        }
        let clipped = clip_text(trimmed, 280);
        if !rendered.iter().any(|existing| existing == &clipped) {
            rendered.push(clipped);
        }
    }
    Ok(rendered)
}

fn is_internal_routing_note(note: &str) -> bool {
    let normalized = normalize_token(note);
    normalized.starts_with("queued for active execution")
        || normalized.starts_with("execution slice hit the turn time budget")
        || normalized.contains("durable continuation")
}

fn task_matches_scope(
    task: &channels::QueueTaskView,
    thread_key: Option<&str>,
    workspace_root: Option<&str>,
) -> bool {
    if let Some(thread_key) = thread_key {
        if task.thread_key == thread_key {
            return true;
        }
    }
    if let (Some(lhs), Some(rhs)) = (
        workspace_root
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        task.workspace_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    ) {
        if lhs == rhs {
            return true;
        }
    }
    false
}

fn task_matches_blocked_labels(task: &channels::QueueTaskView, blocked_labels: &[&str]) -> bool {
    let haystack = format!("{}\n{}", task.title, task.prompt).to_ascii_lowercase();
    blocked_labels
        .iter()
        .any(|label| haystack.contains(&label.to_ascii_lowercase()))
}

fn find_superseding_corrective_queue_task(
    root: &Path,
    thread_key: Option<&str>,
    workspace_root: Option<&str>,
    blocked_labels: &[&str],
) -> Result<Option<channels::QueueTaskView>> {
    let tasks =
        channels::list_queue_tasks(root, &["pending".to_string(), "leased".to_string()], 128)?;
    Ok(tasks
        .into_iter()
        .filter(|task| task_matches_scope(task, thread_key, workspace_root))
        .filter(|task| !task_matches_blocked_labels(task, blocked_labels))
        .max_by_key(|task| queue_priority_rank(&task.priority)))
}

fn self_work_has_explicit_supersession(item: &tickets::TicketSelfWorkItemView) -> bool {
    let haystack = format!("{}\n{}", item.title, item.body_text).to_ascii_lowercase();
    haystack.contains("superseded by canonical mission conversation")
        || haystack.contains("superseded by clean probe")
}

fn watchdog_generated_mission_follow_up(item: &tickets::TicketSelfWorkItemView) -> bool {
    if item.kind != "mission-follow-up" {
        return false;
    }
    if item.body_text.contains("Mission continuity watchdog:") {
        return true;
    }
    item.metadata
        .get("dedupe_key")
        .and_then(|value| value.as_str())
        .map(|value| value.starts_with("mission-watchdog:"))
        .unwrap_or(false)
}

fn suppress_self_work_reason(
    root: &Path,
    item: &tickets::TicketSelfWorkItemView,
) -> Result<Option<String>> {
    if self_work_has_explicit_supersession(item) {
        return Ok(Some(
            "suppressed because the work item was explicitly marked as superseded".to_string(),
        ));
    }

    let thread_key = ticket_self_work_thread_key(item);
    let workspace_root = ticket_self_work_workspace_root(item);
    let blocked_labels: &[&str] = match item.kind.as_str() {
        "review-rework" => &["review rework"],
        PLATFORM_IMPLEMENTATION_KIND => &["platform implementation reset", "review rework"],
        STRATEGIC_DIRECTION_KIND => &["strategic direction setup"],
        CTO_DRIFT_KIND => &["cto operating drift correction"],
        "mission-follow-up" if watchdog_generated_mission_follow_up(item) => &[
            "continue mission",
            "monitor ",
            "approval",
            "watch for effective",
        ],
        _ => &[],
    };
    if blocked_labels.is_empty() {
        return Ok(None);
    }
    if let Some(task) = find_superseding_corrective_queue_task(
        root,
        Some(&thread_key),
        workspace_root.as_deref(),
        blocked_labels,
    )? {
        if queue_priority_rank(&task.priority)
            >= queue_priority_rank(&ticket_self_work_priority(item))
        {
            return Ok(Some(format!(
                "suppressed because runnable corrective work already exists: {} ({})",
                task.title, task.message_key
            )));
        }
    }
    Ok(None)
}

fn maybe_skip_superseded_self_work_prompt(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    job: &QueuedPrompt,
) -> Result<bool> {
    let Some(work_id) = job.ticket_self_work_id.as_deref() else {
        return Ok(false);
    };
    let Some(item) = tickets::load_ticket_self_work_item(root, work_id)? else {
        return Ok(false);
    };
    let Some(reason) = suppress_self_work_reason(root, &item)? else {
        return Ok(false);
    };

    if !job.leased_message_keys.is_empty() {
        let _ = channels::ack_leased_messages(root, &job.leased_message_keys, "cancelled");
    }
    if !job.leased_ticket_event_keys.is_empty() {
        let _ = tickets::ack_leased_ticket_events(root, &job.leased_ticket_event_keys, "blocked");
    }
    supersede_ticket_self_work_item(
        root,
        work_id,
        &format!("Closed without execution because the work was superseded: {reason}"),
    );

    let mut next_prompt = None;
    {
        let mut shared = lock_shared_state(state);
        shared.busy = false;
        shared.current_goal_preview = None;
        shared.active_source_label = None;
        shared.last_completed_at = Some(now_iso_string());
        shared.last_progress_epoch_secs = current_epoch_secs();
        shared.last_reply_chars = None;
        shared.last_error = None;
        release_leased_keys_locked(
            &mut shared,
            &job.leased_message_keys,
            &job.leased_ticket_event_keys,
        );
        push_event_locked(
            &mut shared,
            format!(
                "Skipped superseded self-work {} [{}]: {}",
                work_id, item.kind, reason
            ),
        );
        if runtime_blocker_backoff_remaining_secs(&shared).is_none() {
            next_prompt = maybe_start_next_queued_prompt_locked(root, &mut shared);
        }
    }
    if let Some(queued) = next_prompt {
        start_prompt_worker(root.to_path_buf(), state.clone(), queued);
    }
    Ok(true)
}

fn queue_ticket_self_work_item(
    root: &Path,
    item: &tickets::TicketSelfWorkItemView,
) -> Result<Option<channels::QueueTaskView>> {
    queue_ticket_self_work_item_ignoring(root, item, &[])
}

fn queue_ticket_self_work_item_ignoring(
    root: &Path,
    item: &tickets::TicketSelfWorkItemView,
    ignored_message_keys: &[String],
) -> Result<Option<channels::QueueTaskView>> {
    let thread_key = ticket_self_work_thread_key(item);
    if let Some(existing) = find_runnable_self_work_task_ignoring(root, item, ignored_message_keys)?
    {
        return Ok(Some(existing));
    }
    if runnable_thread_task_exists_ignoring(root, &thread_key, ignored_message_keys)? {
        return Ok(None);
    }
    let mut extra_metadata = serde_json::json!({
        "ticket_self_work_id": item.work_id.clone(),
        "ticket_self_work_kind": item.kind.clone(),
        "ticket_self_work_source_system": item.source_system.clone(),
    });
    merge_metadata_value(&mut extra_metadata, ticket_self_work_queue_metadata(item));
    let queue_task = channels::create_queue_task_with_metadata(
        root,
        channels::QueueTaskCreateRequest {
            title: item.title.trim().to_string(),
            prompt: render_ticket_self_work_prompt(root, item),
            thread_key: thread_key.clone(),
            workspace_root: ticket_self_work_workspace_root(item),
            priority: ticket_self_work_priority(item),
            suggested_skill: item.suggested_skill.clone(),
            parent_message_key: ticket_self_work_parent_message_key(item),
            extra_metadata: Some(extra_metadata),
        },
    )?;
    let note = format!(
        "Queued for active execution on thread `{}` as queue task `{}`.",
        thread_key, queue_task.title
    );
    let _ = tickets::transition_ticket_self_work_item(
        root,
        &item.work_id,
        "queued",
        "ctox-service",
        Some(&note),
        "internal",
    );
    Ok(Some(queue_task))
}

fn find_runnable_self_work_task_ignoring(
    root: &Path,
    item: &tickets::TicketSelfWorkItemView,
    ignored_message_keys: &[String],
) -> Result<Option<channels::QueueTaskView>> {
    let dedupe_key = ticket_self_work_dedupe_key(item);
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT m.message_key
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'queue'
          AND m.direction = 'inbound'
          AND lower(COALESCE(r.route_status, 'pending')) IN ('pending', 'leased')
          AND (
                json_extract(m.metadata_json, '$.ticket_self_work_id') = ?1
             OR (?2 IS NOT NULL AND json_extract(m.metadata_json, '$.dedupe_key') = ?2)
          )
        ORDER BY
            CASE COALESCE(r.route_status, 'pending')
                WHEN 'pending' THEN 0
                WHEN 'leased' THEN 1
                ELSE 9
            END ASC,
            m.external_created_at ASC,
            m.observed_at ASC
        LIMIT 16
        "#,
    )?;
    let rows = statement.query_map(
        params![item.work_id.as_str(), dedupe_key.as_deref()],
        |row| row.get::<_, String>(0),
    )?;
    let message_keys = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    drop(conn);
    for message_key in message_keys {
        if ignored_message_keys.iter().any(|key| key == &message_key) {
            continue;
        }
        if let Some(task) = channels::load_queue_task(root, &message_key)? {
            return Ok(Some(task));
        }
    }
    Ok(None)
}

fn find_runnable_thread_task_ignoring(
    root: &Path,
    thread_key: &str,
    ignored_message_keys: &[String],
) -> Result<Option<channels::QueueTaskView>> {
    let tasks =
        channels::list_queue_tasks(root, &["pending".to_string(), "leased".to_string()], 64)?;
    Ok(tasks.into_iter().find(|task| {
        task.thread_key == thread_key
            && !ignored_message_keys
                .iter()
                .any(|key| key == &task.message_key)
    }))
}

fn requeue_review_rejected_self_work(
    root: &Path,
    work_id: &str,
    summary: &str,
) -> Result<Option<channels::QueueTaskView>> {
    if let Some(note) = runtime_api_retry_review_rejection_block_note(root, work_id, summary)? {
        block_self_work_queue_tasks_for_work(root, work_id, &note)?;
        block_ticket_self_work_item(root, work_id, &note);
        return Ok(None);
    }
    if let Some(note) = review_checkpoint_loop_block_note(root, work_id, summary)? {
        block_self_work_queue_tasks_for_work(root, work_id, &note)?;
        if !review_checkpoint_loop_block_already_active(root, work_id)? {
            block_ticket_self_work_item(root, work_id, &note);
        }
        return Ok(None);
    }
    if let Some(note) = founder_rework_review_loop_block_note(root, work_id, summary)? {
        block_founder_rework_queue_tasks_for_work(root, work_id, &note)?;
        if !founder_rework_loop_block_already_active(root, work_id)? {
            block_ticket_self_work_item(root, work_id, &note);
        }
        return Ok(None);
    }
    let note = format!(
        "External review rejected the last slice. Summary: {}. Resume this existing work item, consult the persisted review verdict/evidence, and address the failed gates before closing it.",
        clip_text(summary, 220)
    );
    let item = tickets::transition_ticket_self_work_item(
        root,
        work_id,
        "queued",
        "ctox-review",
        Some(&note),
        "internal",
    )?;
    if let Some(reason) = suppress_self_work_reason(root, &item)? {
        supersede_ticket_self_work_item(
            root,
            work_id,
            &format!(
                "Closed after review rejection because the work was superseded during requeue: {reason}"
            ),
        );
        return Ok(None);
    }
    queue_ticket_self_work_item(root, &item)
}

fn runtime_api_retry_review_rejection_block_note(
    root: &Path,
    work_id: &str,
    summary: &str,
) -> Result<Option<String>> {
    let Some(item) = tickets::load_ticket_self_work_item(root, work_id)? else {
        return Ok(None);
    };
    if item.kind != RUNTIME_API_RETRY_KIND {
        return Ok(None);
    }
    Ok(Some(format!(
        "Dieses Runtime-API-Retry-Work-Item `{}` wurde vom Review-Checkpoint abgelehnt. Runtime-API-Retry ist nur fuer die Fortsetzung nach transienten API-Fehlern gedacht und darf nach einem Review-Reject nicht automatisch erneut starten; sonst kann der Harness dieselbe Multi-Turn-Arbeit wiederholt ausfuehren und Tokens verbrennen. Lege eine frische fachliche Aufgabe oder einen exakten reviewed-send-Fortsetzungsauftrag mit belastbarer Evidenz an. Letzter Review-Hinweis: {}",
        item.work_id,
        clip_text(summary, 220)
    )))
}

fn review_checkpoint_requeue_block_threshold() -> usize {
    match std::env::var("CTOX_REVIEW_CHECKPOINT_REQUEUE_BLOCK_THRESHOLD") {
        Ok(value) => match value.trim().parse::<usize>() {
            Ok(parsed) if parsed > 0 => parsed.min(MAX_REVIEW_CHECKPOINT_REQUEUE_BLOCK_THRESHOLD),
            _ => REVIEW_CHECKPOINT_REQUEUE_BLOCK_THRESHOLD,
        },
        Err(_) => REVIEW_CHECKPOINT_REQUEUE_BLOCK_THRESHOLD,
    }
}

fn review_checkpoint_loop_block_note(
    root: &Path,
    work_id: &str,
    summary: &str,
) -> Result<Option<String>> {
    let Some(item) = tickets::load_ticket_self_work_item(root, work_id)? else {
        return Ok(None);
    };
    let attempts = review_checkpoint_requeue_attempt_count(root, work_id)?;
    let threshold = review_checkpoint_requeue_block_threshold();
    if attempts < threshold {
        return Ok(None);
    }
    Ok(Some(format!(
        "Dieses Work Item `{}` ({}) wurde {attempts} Mal vom Review-Checkpoint zur Nacharbeit zurueckgegeben. Die Requeue-Schranke ({threshold}) ist erreicht; der Harness blockt weitere automatische Review-Requeues, damit kein unendlicher Self-Rework-Loop entstehen kann. Fuehre erst neue belastbare Arbeit oder Evidenz in einem separaten fachlichen Task zu. Letzter Review-Hinweis: {}",
        item.work_id,
        item.kind,
        clip_text(summary, 220)
    )))
}

fn review_checkpoint_loop_block_already_active(root: &Path, work_id: &str) -> Result<bool> {
    let Some(item) = tickets::load_ticket_self_work_item(root, work_id)? else {
        return Ok(false);
    };
    if item.state != "blocked" {
        return Ok(false);
    }
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let exists: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ticket_self_work_notes
        WHERE work_id = ?1
          AND body_text LIKE 'Dieses Work Item `% wurde % Mal vom Review-Checkpoint zur Nacharbeit zurueckgegeben.%'
        LIMIT 1
        "#,
        params![work_id],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}

fn review_checkpoint_requeue_attempt_count(root: &Path, work_id: &str) -> Result<usize> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ticket_self_work_notes
        WHERE work_id = ?1
          AND body_text LIKE 'External review rejected the last slice.%'
        "#,
        params![work_id],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as usize)
}

fn requeue_continue_requested_self_work(
    root: &Path,
    work_id: &str,
    summary: &str,
) -> Result<Option<channels::QueueTaskView>> {
    let note = format!(
        "Die letzte Ausfuehrung ist noch nicht fertig und hat konkrete naechste Schritte genannt. Setze genau diese Arbeit fort. Wenn danach fertig, liefere eine abgeschlossene Zusammenfassung; schlage nur dann erneut Fortsetzung vor, wenn konkrete Pflichtpunkte offen bleiben. Kontext: {}",
        clip_text(summary, 420)
    );
    let item = tickets::transition_ticket_self_work_item(
        root,
        work_id,
        "queued",
        "ctox-continuation",
        Some(&note),
        "internal",
    )?;
    if let Some(reason) = suppress_self_work_reason(root, &item)? {
        supersede_ticket_self_work_item(
            root,
            work_id,
            &format!("Closed instead of auto-continuing because the work was superseded: {reason}"),
        );
        return Ok(None);
    }
    queue_ticket_self_work_item(root, &item)
}

fn requeue_runtime_failed_self_work(
    root: &Path,
    work_id: &str,
    note: &str,
) -> Result<Option<channels::QueueTaskView>> {
    let item = tickets::transition_ticket_self_work_item(
        root,
        work_id,
        "queued",
        "ctox-service",
        Some(note),
        "internal",
    )?;
    if let Some(reason) = suppress_self_work_reason(root, &item)? {
        supersede_ticket_self_work_item(
            root,
            work_id,
            &format!("Closed instead of runtime retry because the work was superseded: {reason}"),
        );
        return Ok(None);
    }
    queue_ticket_self_work_item(root, &item)
}

fn founder_rework_review_loop_block_note(
    root: &Path,
    work_id: &str,
    summary: &str,
) -> Result<Option<String>> {
    let Some(item) = tickets::load_ticket_self_work_item(root, work_id)? else {
        return Ok(None);
    };
    if item.kind != FOUNDER_COMMUNICATION_REWORK_KIND {
        return Ok(None);
    }
    let active_attempts = founder_rework_queue_attempt_count(root, work_id)?;
    if active_attempts < FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD {
        return Ok(None);
    }
    Ok(Some(format!(
        "Diese Founder-Antwort wurde {active_attempts} Mal ohne neue belastbare Grundlage erneut vom Review zurueckgewiesen. Stoppe diese Kommunikationsschleife jetzt: arbeite zuerst die fachliche Grundlage ab, sammle neue Evidenz und erstelle danach eine frische Antwort im selben Thread. Letzter Review-Hinweis: {}",
        clip_text(summary, 220)
    )))
}

fn founder_rework_loop_block_already_active(root: &Path, work_id: &str) -> Result<bool> {
    let Some(item) = tickets::load_ticket_self_work_item(root, work_id)? else {
        return Ok(false);
    };
    if item.state != "blocked" {
        return Ok(false);
    }
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let exists: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ticket_self_work_notes
        WHERE work_id = ?1
          AND body_text LIKE 'Diese Founder-Antwort wurde % ohne neue belastbare Grundlage erneut vom Review zurueckgewiesen.%'
        LIMIT 1
        "#,
        params![work_id],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}

fn founder_rework_queue_attempt_count(root: &Path, work_id: &str) -> Result<usize> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'queue'
          AND m.direction = 'inbound'
          AND json_extract(m.metadata_json, '$.ticket_self_work_id') = ?1
          AND (
                json_extract(m.metadata_json, '$.ticket_self_work_kind') = ?2
             OR m.subject LIKE 'Founder communication rework:%'
          )
          AND lower(COALESCE(r.route_status, 'pending')) IN (
                'pending', 'leased', 'review_rework', 'blocked', 'failed'
          )
        "#,
        params![work_id, FOUNDER_COMMUNICATION_REWORK_KIND],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as usize)
}

fn block_founder_rework_queue_tasks_for_work(
    root: &Path,
    work_id: &str,
    note: &str,
) -> Result<usize> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT m.message_key
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'queue'
          AND m.direction = 'inbound'
          AND json_extract(m.metadata_json, '$.ticket_self_work_id') = ?1
          AND (
                json_extract(m.metadata_json, '$.ticket_self_work_kind') = ?2
             OR m.subject LIKE 'Founder communication rework:%'
          )
          AND lower(COALESCE(r.route_status, 'pending')) IN (
                'pending', 'leased', 'review_rework'
          )
        "#,
    )?;
    let rows = statement.query_map(params![work_id, FOUNDER_COMMUNICATION_REWORK_KIND], |row| {
        row.get::<_, String>(0)
    })?;
    let message_keys = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    drop(conn);

    let mut blocked = 0usize;
    for message_key in message_keys {
        channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key,
                route_status: Some("blocked".to_string()),
                status_note: Some(note.to_string()),
                ..Default::default()
            },
        )?;
        blocked += 1;
    }
    Ok(blocked)
}

fn block_self_work_queue_tasks_for_work(root: &Path, work_id: &str, note: &str) -> Result<usize> {
    let db_path = crate::paths::core_db(&root);
    let conn = channels::open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT m.message_key
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'queue'
          AND m.direction = 'inbound'
          AND json_extract(m.metadata_json, '$.ticket_self_work_id') = ?1
          AND lower(COALESCE(r.route_status, 'pending')) IN (
                'pending', 'leased', 'review_rework'
          )
        "#,
    )?;
    let rows = statement.query_map(params![work_id], |row| row.get::<_, String>(0))?;
    let message_keys = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    drop(conn);

    let mut blocked = 0usize;
    for message_key in message_keys {
        channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key,
                route_status: Some("blocked".to_string()),
                status_note: Some(note.to_string()),
                ..Default::default()
            },
        )?;
        blocked += 1;
    }
    Ok(blocked)
}

fn create_self_work_backed_queue_task(
    root: &Path,
    request: DurableSelfWorkQueueRequest,
) -> Result<channels::QueueTaskView> {
    create_self_work_backed_queue_task_ignoring(root, request, &[])
}

fn create_self_work_backed_queue_task_ignoring(
    root: &Path,
    request: DurableSelfWorkQueueRequest,
    ignored_message_keys: &[String],
) -> Result<channels::QueueTaskView> {
    let DurableSelfWorkQueueRequest {
        kind,
        title,
        prompt,
        thread_key,
        workspace_root,
        priority,
        suggested_skill,
        parent_message_key,
        metadata,
    } = request;
    let mut self_work_metadata = serde_json::json!({
        "thread_key": thread_key,
        "workspace_root": workspace_root,
        "priority": priority,
        "skill": suggested_skill,
        "parent_message_key": parent_message_key,
    });
    merge_metadata_value(&mut self_work_metadata, metadata);
    let item = tickets::put_ticket_self_work_item(
        root,
        tickets::TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind,
            title,
            body_text: prompt,
            state: "open".to_string(),
            metadata: self_work_metadata,
        },
        true,
    )?;
    if item.assigned_to.as_deref() != Some("self") {
        let _ = tickets::assign_ticket_self_work_item(
            root,
            &item.work_id,
            "self",
            "ctox-service",
            Some("durable complex follow-up for CTOX"),
        );
    }
    if let Some(view) = queue_ticket_self_work_item_ignoring(root, &item, ignored_message_keys)? {
        return Ok(view);
    }
    find_runnable_thread_task_ignoring(
        root,
        &ticket_self_work_thread_key(&item),
        ignored_message_keys,
    )?
    .context("failed to queue durable self-work follow-up")
}

fn close_ticket_self_work_item(root: &Path, work_id: &str, note: &str) {
    let _ = tickets::transition_ticket_self_work_item(
        root,
        work_id,
        "closed",
        "ctox-service",
        Some(note),
        "internal",
    );
}

fn supersede_ticket_self_work_item(root: &Path, work_id: &str, note: &str) {
    let _ = tickets::transition_ticket_self_work_item(
        root,
        work_id,
        "superseded",
        "ctox-service",
        Some(note),
        "internal",
    );
}

fn block_ticket_self_work_item(root: &Path, work_id: &str, note: &str) {
    let _ = tickets::transition_ticket_self_work_item(
        root,
        work_id,
        "blocked",
        "ctox-service",
        Some(note),
        "internal",
    );
}

fn normalize_token(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_lowercase()
}

fn preview_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(96)
        .collect()
}

fn enrich_inbound_prompt(
    root: &Path,
    settings: &BTreeMap<String, String>,
    message: &channels::RoutedInboundMessage,
    prompt_body: &str,
) -> String {
    if message.channel == "email" {
        let policy = channels::classify_email_sender(settings, &message.sender_address);
        let sender = display_inbound_sender(message);
        let subject = if message.subject.trim().is_empty() {
            "(ohne Betreff)"
        } else {
            message.subject.trim()
        };
        let reply_target = if message.sender_address.trim().is_empty() {
            "(unknown sender)"
        } else {
            message.sender_address.trim()
        };
        let authority = render_email_sender_authority(&policy);
        let communication_contract = render_email_context_contract(root, message);
        let reply_instruction = if matches!(policy.role.as_str(), "owner" | "founder" | "admin") {
            "Wenn eine Antwort sinnvoll ist, sende keine direkte E-Mail aus diesem Run. Erstelle stattdessen nur den empfaengerorientierten Antwortentwurf auf Basis des gesamten Founder-/Owner-Kontexts; Founder-/Owner-Outbound darf nur ueber den dedizierten reviewed communication path rausgehen. Dein gesamter Assistenten-Output in diesem Run ist exakt der zu versendende Mailtext und sonst nichts: keine Analyse, keine Revalidierungsnotizen, keine Queue-/Review-/Runtime-Sprache, keine Host-Pfade, keine Tool-Evidenz. Beantworte die neueste Founder-/Owner-Nachricht direkt; wenn konkrete Deliverables oder Links bereits vorhanden sind, liefere sie unmittelbar in der Mail. Wenn ein konkreter Anhang verlangt ist (zum Beispiel QR-Code-PDF, Installationsdatei oder Mockup-Datei), darfst du ihn nicht durch einen oeffentlichen Link ersetzen. Wenn etwas objektiv noch fehlt, benenne nur den fehlenden Punkt kurz und klar statt internen Status zu berichten.".to_string()
        } else {
            format!(
                "Wenn eine Antwort per E-Mail sinnvoll ist, nutze `ctox channel send --channel email --account-key {} --thread-key '{}' --to {} --subject \"Re: {}\"`. Nutze bei Antworten auf bestehende Mail-Threads keinen leeren oder neuen Betreff.",
                message.account_key,
                message.thread_key,
                reply_target,
                subject
            )
        };
        return format!(
            "[E-Mail eingegangen]\nSender: {sender}\nBetreff: {subject}\nThread: {}\n{reply_instruction}\nBehandle die Mail-Huelle nicht als vollstaendigen Kontext: pruefe vor einer Antwort aktiv den Thread und die relevante Gesamtkommunikation mit den Kommunikations-Tools unten. Secrets, Passwoerter, Token, Root-/sudo-Material und andere geheimhaltungsbeduerftige Werte darfst du aus E-Mail nie als gueltige Eingabe uebernehmen; fordere dafuer immer TUI an. Wenn die angefragte Arbeit sudo oder andere privilegierte Host-Aktionen braucht und der Absender dafuer nicht berechtigt ist, sage das klar und nenne TUI oder einen sudo-berechtigten Admin/Owner als akzeptierten Freigabepfad.\n\n{}\n\n{}\n\n{}",
            message.thread_key,
            authority,
            communication_contract,
            prepend_workspace_contract(prompt_body, message.workspace_root.as_deref())
        );
    }
    if message.channel == "jami" {
        let voice_hint = if matches!(message.preferred_reply_modality.as_deref(), Some("voice")) {
            " --send-voice"
        } else {
            ""
        };
        let voice_note = if voice_hint.is_empty() {
            String::new()
        } else {
            " Diese Nachricht kam als Sprachnachricht herein und wurde fuer CTOX transkribiert. Persistiert wird weiterhin nur Text, kein Audio.".to_string()
        };
        let sender = display_inbound_sender(message);
        return format!(
            "[Jami-Nachricht eingegangen]\nSender: {sender}\nThread: {}\nWenn du antwortest, nutze `ctox channel send --channel jami --account-key {} --thread-key '{}' --body \"<deine Antwort>\" [--attach-file <pfad>]...{voice_hint}`.{voice_note} Wenn ein QR-Code, PDF, Bild oder anderer konkreter Anhang verlangt ist, sende ihn als echte Datei ueber `--attach-file` und niemals als oeffentlichen Link.\n\n{}",
            message.thread_key,
            message.account_key,
            message.thread_key,
            prepend_workspace_contract(prompt_body, message.workspace_root.as_deref())
        );
    }
    if message.channel == "teams" {
        let sender = display_inbound_sender(message);
        let subject_line = if message.subject.trim().is_empty() {
            String::new()
        } else {
            format!("\nBetreff: {}", message.subject.trim())
        };
        return format!(
            "[Teams-Nachricht eingegangen]\nSender: {sender}{subject_line}\nThread: {}\nWenn du antwortest, nutze `ctox channel send --channel teams --account-key {} --thread-key '{}' --body \"<deine Antwort>\"`. Der Teams-Adapter sendet ueber Microsoft Graph in den konfigurierten Chat oder Channel-Thread; erfinde keine Empfaengeradresse und wechsle fuer Live-Meeting-Chat nur auf den `meeting`-Kanal, wenn die Nachricht aus einer aktiven Meeting-Session stammt.\n\n{}",
            message.thread_key,
            message.account_key,
            message.thread_key,
            prepend_workspace_contract(prompt_body, message.workspace_root.as_deref())
        );
    }
    if message.channel == "whatsapp" {
        let sender = display_inbound_sender(message);
        return format!(
            "[WhatsApp-Nachricht eingegangen]\nSender: {sender}\nThread: {}\nWenn du antwortest, nutze `ctox channel send --channel whatsapp --account-key {} --thread-key '{}' --body \"<deine Antwort>\"` [--attach-file <pfad>].... Antworte auf diesem Kanal kurz und direkt; wenn ein konkreter Anhang verlangt ist, sende ihn als echte Datei ueber `--attach-file`.\n\n{}",
            message.thread_key,
            message.account_key,
            message.thread_key,
            prepend_workspace_contract(prompt_body, message.workspace_root.as_deref())
        );
    }
    if message.channel == "meeting" {
        let sender = display_inbound_sender(message);
        let session_id = &message.thread_key; // thread_key == session_id
        let provider = message
            .metadata
            .get("provider")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let is_mention =
            crate::communication::meeting_native::MeetingSession::is_mention(prompt_body);
        let mention_hint = if is_mention {
            format!(
                " Du wurdest per @CTOX erwaehnt — antworte im Meeting-Chat.\n\
                 Nutze `meeting_get_transcript` fuer das vollstaendige Transkript.\n\
                 Nutze `meeting_send_chat` mit session_id `{session_id}` um zu antworten.\n\
                 Halte deine Antwort kurz (1-3 Saetze)."
            )
        } else {
            String::new()
        };
        return format!(
            "[Meeting-Chat-Nachricht eingegangen]\n\
             Provider: {provider}\n\
             Sender: {sender}\n\
             Session: {session_id}\n\
             Wenn du im Meeting-Chat antworten willst, nutze `ctox channel send --channel meeting --thread-key '{session_id}' --body \"<deine Antwort>\"`.{mention_hint}\n\n{}",
            prepend_workspace_contract(prompt_body, message.workspace_root.as_deref())
        );
    }
    prepend_workspace_contract(prompt_body, message.workspace_root.as_deref())
}

fn prepend_workspace_contract(prompt: &str, workspace_root: Option<&str>) -> String {
    let Some(workspace_root) = workspace_root
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return prompt.to_string();
    };
    if prompt
        .trim_start()
        .starts_with("Work only inside this workspace:")
    {
        return prompt.to_string();
    }
    format!(
        "Work only inside this workspace:\n{workspace_root}\n\nExecution contract: If this request asks for files, commands, runtime state, tickets, benchmarks, or verification, do the work with the available terminal/shell tools. A plan, code block, or status sentence is not execution.\n\n{prompt}"
    )
}

fn render_email_context_contract(root: &Path, message: &channels::RoutedInboundMessage) -> String {
    let sender = if message.sender_address.trim().is_empty() {
        "(unknown sender)"
    } else {
        message.sender_address.trim()
    };
    let mut query_parts = Vec::new();
    if !message.subject.trim().is_empty() {
        query_parts.push(message.subject.trim());
    }
    if !message.preview.trim().is_empty() {
        query_parts.push(message.preview.trim());
    }
    let search_hint = if query_parts.is_empty() {
        sender.to_string()
    } else {
        format!("{sender} {}", query_parts.join(" "))
    };
    let db_path = crate::paths::core_db(&root);
    let lcm_path = crate::paths::core_db(&root);
    let lines = vec![
        "[Kommunikationskontext aktiv pruefen]".to_string(),
        "Vor einer Antwort nicht nur auf diese Mail-Huelle verlassen.".to_string(),
        format!(
            "- Erst den relevanten Zustand rekonstruieren: `ctox channel context --db {} --thread-key '{}' --query '{}' --sender '{}' --limit 12`",
            db_path.display(),
            message.thread_key,
            search_hint.replace('\'', " "),
            sender.replace('\'', " ")
        ),
        format!(
            "- Thread pruefen: `ctox channel history --db {} --thread-key '{}' --limit 12`",
            db_path.display(),
            message.thread_key
        ),
        format!(
            "- Verwandte Kommunikation suchen: `ctox channel search --db {} --query '{}' --limit 12`",
            db_path.display(),
            search_hint.replace('\'', " ")
        ),
        format!(
            "- Falls TUI-/Agentenentscheidungen relevant sein koennten, in LCM suchen: `ctox lcm-grep {} all messages smart '{}' 12`",
            lcm_path.display(),
            sender.replace('\'', " ")
        ),
        "Erst danach entscheiden, ob fruehere Zusagen, Blocker, Freigaben, Nachfragen oder offene Arbeiten die neue Antwort aendern.".to_string(),
    ];
    lines.join("\n")
}

fn sync_configured_channels(root: &Path, settings: &BTreeMap<String, String>) {
    let _ = communication_adapters::email().service_sync(root, settings);
    let _ = communication_adapters::jami().service_sync(root, settings);
    let _ = communication_adapters::meeting().service_sync(root, settings);
    let _ = communication_adapters::teams().service_sync(root, settings);
    let _ = communication_adapters::whatsapp().service_sync(root, settings);
}

fn sync_configured_tickets(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    settings: &BTreeMap<String, String>,
) -> HashSet<String> {
    let mut ok_sources = HashSet::new();
    for result in tickets::sync_configured_ticket_systems(root, settings) {
        if result.ok {
            ok_sources.insert(result.system);
            continue;
        }
        let system = result.system.clone();
        let error = result
            .error
            .as_deref()
            .unwrap_or("unknown ticket sync error");
        let idempotence_key = format!(
            "ticket-sync-failed:{}:{}",
            system,
            normalize_token(&clip_text(error, 96))
        );
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "ticket_adapter_sync",
                conversation_id: None,
                severity: "warning",
                reason: error,
                action_taken: "recorded ticket sync failure and skipped dispatch from this source for this cycle",
                details: serde_json::json!({
                    "system": system.clone(),
                }),
                idempotence_key: Some(&idempotence_key),
            },
        );
        push_event(
            state,
            format!("Ticket sync failed for {system}: {}", clip_text(error, 180)),
        );
    }
    ok_sources
}

fn render_ticket_prompt(root: &Path, event: &tickets::RoutedTicketEvent) -> String {
    let dry_run =
        serde_json::to_string_pretty(&event.dry_run_artifact).unwrap_or_else(|_| "{}".to_string());
    let ctox = preferred_ctox_executable(root)
        .unwrap_or_else(|_| std::env::current_exe().unwrap_or_else(|_| root.join("ctox")))
        .display()
        .to_string();
    let source_skill_query = format!(
        "{}. {}",
        event.title.replace('"', "'"),
        clip_text(&event.body_text.replace('"', "'").replace('\n', " "), 220)
    );
    format!(
        "[Ticket-Ereignis]\nSystem: {system}\nTicket: {ticket_key}\nStatus: {status}\nTitel: {title}\nEvent: {event_type}\nLabel: {label}\nSupport-Modus: {support_mode}\nApproval-Modus: {approval_mode}\nAutonomie: {autonomy_level}\nCase: {case_id}\nDry-Run: {dry_run_id}\n\nZusammenfassung:\n{summary}\n\nEreignistext:\n{body}\n\nVerbindlicher Ablauf:\n- lade und beachte die Ticket-Referenzen, bevor du operative Entscheidungen triffst\n- beginne mit dem vorhandenen Dry-Run-Artefakt; fuehre keine ungebundenen Nebenaktionen aus\n- resolve zuerst den gebundenen Main-Skill fuer dieses Ticket, bevor du eine Antwort oder Aktion ableitest\n- wenn du eine interne Ticketnotiz schreibst, formuliere sie frisch in Desk-Sprache; kopiere keine Skill- oder Query-Ausgabe\n- wenn es ein antwortbarer Supportfall ist, compose zuerst eine Reply-Suggestion und schreibe erst danach bewusst zurueck\n- wenn weitere Aktion noetig ist, halte den Schritt klein, explizit und auditierbar\n- wenn Freigabe fehlt, nutze keinen verdeckten Bypass\n- wenn du schreiben willst, verwende die Ticket-CLI bewusst und nur passend zum Case-Status\n\nDry-Run-Artefakt:\n```json\n{dry_run}\n```\n\nNuetzliche Ticket-Befehle:\n- Desk-Skill ansehen: `{ctox} ticket source-skill-show --system {system}`\n- Desk-Skill abfragen: `{ctox} ticket source-skill-query --system {system} --query \\\"{source_skill_query}\\\" --top-k 1`\n- Main-Skill fuer diesen Case aufloesen: `{ctox} ticket source-skill-resolve --case-id {case_id} --top-k 3`\n- Reply-Suggestion erzeugen: `{ctox} ticket source-skill-compose-reply --case-id {case_id} --send-policy suggestion`\n- Notiz gegen Desk-Skill pruefen: `{ctox} ticket source-skill-review-note --case-id {case_id} --body \\\"<frische interne Notiz>\\\"`\n- Knowledge ansehen: `{ctox} ticket knowledge-list --system {system} --limit 12`\n- Einzelnes Knowledge ansehen: `{ctox} ticket knowledge-show --system {system} --domain <value> --key <value>`\n- Self-Work ansehen: `{ctox} ticket self-work-list --system {system} --limit 12`\n- Case anzeigen: `{ctox} ticket case-show --case-id {case_id}`\n- Freigeben: `{ctox} ticket approve --case-id {case_id} --status approved --decided-by owner`\n- Ablehnen: `{ctox} ticket approve --case-id {case_id} --status rejected --decided-by owner`\n- Ausfuehrung dokumentieren: `{ctox} ticket execute --case-id {case_id} --summary \\\"<kurzer Schritt>\\\"`\n- Verifikation erfassen: `{ctox} ticket verify --case-id {case_id} --status passed --summary \\\"<evidence>\\\"`\n- Oeffentliche Ticketantwort: `{ctox} ticket writeback-comment --case-id {case_id} --body \\\"<reply text>\\\"`\n- Interne Ticketnotiz: `{ctox} ticket writeback-comment --case-id {case_id} --body \\\"<frische interne Notiz>\\\" --internal`\n- Ticket-Status zurueckschreiben: `{ctox} ticket writeback-transition --case-id {case_id} --state \\\"<zielstatus>\\\" --body \\\"<optional text>\\\"`\n- Audit ansehen: `{ctox} ticket audit --ticket-key {ticket_key} --limit 12`\n",
        system = event.source_system,
        ticket_key = event.ticket_key,
        status = event.remote_status,
        title = event.title,
        event_type = event.event_type,
        label = event.label,
        support_mode = event.support_mode,
        approval_mode = event.approval_mode,
        autonomy_level = event.autonomy_level,
        case_id = event.case_id,
        dry_run_id = event.dry_run_id,
        summary = event.summary,
        body = event.body_text,
        source_skill_query = source_skill_query,
    )
}

fn blocked_inbound_reason(
    message: &channels::RoutedInboundMessage,
    settings: &BTreeMap<String, String>,
) -> Option<String> {
    if message.channel != "email" {
        return None;
    }
    let policy = channels::classify_email_sender(settings, &message.sender_address);
    if policy.block_reason.is_some() {
        return policy.block_reason;
    }
    if email_contains_secret_material(message) {
        return Some("secret-bearing input must move to TUI".to_string());
    }
    None
}

fn email_contains_secret_material(message: &channels::RoutedInboundMessage) -> bool {
    let haystack = format!(
        "{}\n{}\n{}",
        message.subject, message.preview, message.body_text
    )
    .to_ascii_lowercase();
    [
        "password:",
        "password=",
        "passwort:",
        "passwort=",
        "token:",
        "api_token=",
        "access_token=",
        "refresh_token=",
        "bearer token",
        "secret:",
        "secret=",
        "api key:",
        "api-key:",
        "api_key=",
        "apikey=",
        "_password=",
        "_api_token=",
        "_secret=",
        "sudo password:",
        "root password:",
    ]
    .iter()
    .any(|marker| contains_secret_assignment(&haystack, marker))
}

fn contains_secret_assignment(haystack: &str, marker: &str) -> bool {
    haystack.match_indices(marker).any(|(idx, _)| {
        let tail = haystack[idx + marker.len()..].trim_start();
        let value = tail.split_whitespace().next().unwrap_or("");
        value.len() >= 4
    })
}

fn display_inbound_sender(message: &channels::RoutedInboundMessage) -> String {
    if !message.sender_display.trim().is_empty() && !message.sender_address.trim().is_empty() {
        return format!(
            "{} <{}>",
            message.sender_display.trim(),
            message.sender_address.trim()
        );
    }
    if !message.sender_address.trim().is_empty() {
        return message.sender_address.trim().to_string();
    }
    if !message.sender_display.trim().is_empty() {
        return message.sender_display.trim().to_string();
    }
    "unknown sender".to_string()
}

fn render_email_sender_authority(policy: &channels::EmailSenderPolicy) -> String {
    let domain = policy
        .allowed_email_domain
        .as_deref()
        .unwrap_or("not configured");
    let admin_scope = if policy.allow_admin_actions {
        "allowed"
    } else {
        "not allowed"
    };
    let sudo_scope = if policy.allow_sudo_actions {
        "allowed"
    } else {
        "not allowed"
    };
    let accepted = if policy.allowed { "yes" } else { "no" };
    let block_reason = policy.block_reason.as_deref().unwrap_or("none");
    format!(
        "[E-Mail Berechtigung]\nAbsenderrolle: {}\nInstruktionsmail akzeptiert: {}\nErlaubte Mail-Domain: {}\nAdmin-Tätigkeiten aus dieser Mail: {}\nPrivilegierte/sudo-Tätigkeiten aus dieser Mail: {}\nSecrets per Mail akzeptieren: never; TUI only\nWenn Arbeit an fehlenden sudo-Rechten scheitert, sage das explizit und nenne den akzeptierten Freigabepfad.\nBlockgrund: {}",
        policy.role, accepted, domain, admin_scope, sudo_scope, block_reason
    )
}

fn push_event(state: &Arc<Mutex<SharedState>>, event: String) {
    let mut shared = lock_shared_state(state);
    push_event_locked(&mut shared, event);
}

fn push_event_locked(shared: &mut SharedState, event: String) {
    if shared.recent_events.len() >= 24 {
        shared.recent_events.pop_front();
    }
    shared.recent_events.push_back(event);
}

fn queue_pressure_active(state: &Arc<Mutex<SharedState>>) -> bool {
    let shared = lock_shared_state(state);
    shared.pending_prompts.len() >= QUEUE_PRESSURE_GUARD_THRESHOLD
}

fn inflight_leased_message_key(state: &Arc<Mutex<SharedState>>, message_key: &str) -> bool {
    let shared = lock_shared_state(state);
    shared.leased_message_keys_inflight.contains(message_key)
}

fn lock_shared_state<'a>(
    state: &'a Arc<Mutex<SharedState>>,
) -> std::sync::MutexGuard<'a, SharedState> {
    match state.lock() {
        Ok(shared) => shared,
        Err(poisoned) => {
            eprintln!("ctox service state mutex was poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

fn install_service_panic_hook() {
    SERVICE_PANIC_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|panic_info| {
            let backtrace = std::backtrace::Backtrace::force_capture();
            eprintln!("ctox service panic: {panic_info}");
            eprintln!("{backtrace}");
        }));
    });
}

fn track_leased_keys_locked(
    shared: &mut SharedState,
    message_keys: &[String],
    ticket_event_keys: &[String],
) {
    for message_key in message_keys {
        shared
            .leased_message_keys_inflight
            .insert(message_key.to_string());
    }
    for event_key in ticket_event_keys {
        shared
            .leased_message_keys_inflight
            .insert(event_key.to_string());
    }
}

fn release_leased_keys_locked(
    shared: &mut SharedState,
    message_keys: &[String],
    ticket_event_keys: &[String],
) {
    for message_key in message_keys {
        shared.leased_message_keys_inflight.remove(message_key);
    }
    for event_key in ticket_event_keys {
        shared.leased_message_keys_inflight.remove(event_key);
    }
}

fn queue_guard_needed(shared: &SharedState) -> bool {
    shared.pending_prompts.len() >= QUEUE_PRESSURE_GUARD_THRESHOLD
}

fn queue_guard_present(shared: &SharedState) -> bool {
    shared.active_source_label.as_deref() == Some(QUEUE_GUARD_SOURCE_LABEL)
        || shared
            .pending_prompts
            .iter()
            .any(|prompt| prompt.source_label == QUEUE_GUARD_SOURCE_LABEL)
}

fn ensure_queue_guard_locked(root: &Path, shared: &mut SharedState) {
    if !queue_guard_needed(shared) || queue_guard_present(shared) {
        return;
    }
    let pending = shared.pending_prompts.len();
    let guard_prompt = build_queue_guard_prompt(root, pending);
    shared.pending_prompts.push_front(QueuedPrompt {
        prompt: guard_prompt.clone(),
        goal: guard_prompt,
        preview: "Queue pressure guard".to_string(),
        source_label: QUEUE_GUARD_SOURCE_LABEL.to_string(),
        suggested_skill: None,
        leased_message_keys: Vec::new(),
        leased_ticket_event_keys: Vec::new(),
        thread_key: None,
        workspace_root: None,
        ticket_self_work_id: None,
        outbound_email: None,
        outbound_anchor: None,
    });
    if let Err(err) = governance::record_event(
        root,
        governance::GovernanceEventRequest {
            mechanism_id: "queue_pressure_guard",
            conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
            severity: "warning",
            reason: "pending prompt pressure crossed the queue guard threshold",
            action_taken: "inserted a queue pressure guard slice at the front of the queue",
            details: serde_json::json!({
                "pending": pending,
                "threshold": QUEUE_PRESSURE_GUARD_THRESHOLD,
            }),
            idempotence_key: None,
        },
    ) {
        push_event_locked(
            shared,
            format!("Queue pressure guard event persistence failed: {err}"),
        );
    }
    push_event_locked(
        shared,
        format!(
            "Inserted queue pressure guard before {} queued prompt(s)",
            pending
        ),
    );
}

fn maybe_enqueue_timeout_continuation(
    root: &Path,
    job: &QueuedPrompt,
    blocker: &str,
) -> Result<Option<String>> {
    if !is_turn_timeout_blocker(blocker) {
        return Ok(None);
    }
    if should_queue_durable_artifact_timeout_recovery(job) {
        return queue_durable_artifact_timeout_recovery(root, job, blocker);
    }
    let _ = governance::record_event(
        root,
        governance::GovernanceEventRequest {
            mechanism_id: "turn_timeout_continuation",
            conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
            severity: "error",
            reason: "the agent turn hit the runtime time budget",
            action_taken:
                "suppressed timeout continuation spawn; original queue scope must retry or defer",
            details: serde_json::json!({
                "source_label": job.source_label,
                "thread_key": job.thread_key.clone(),
                "ticket_self_work_id": job.ticket_self_work_id.clone(),
                "leased_message_keys": job.leased_message_keys.clone(),
                "blocker": clip_text(blocker, 180),
            }),
            idempotence_key: Some(&format!(
                "timeout-continuation-suppressed:{}:{}",
                job.thread_key.as_deref().unwrap_or(job.goal.as_str()),
                job.leased_message_keys
                    .first()
                    .map(String::as_str)
                    .or(job.ticket_self_work_id.as_deref())
                    .unwrap_or(job.goal.as_str()),
            )),
        },
    );
    Ok(None)
}

fn should_queue_durable_artifact_timeout_recovery(job: &QueuedPrompt) -> bool {
    if job.outbound_email.is_some()
        || founder_email_reply_message_key(job).is_some()
        || is_founder_or_owner_email_job(job)
        || job.ticket_self_work_id.is_some()
        || is_legacy_timeout_continuation_job(job)
    {
        return false;
    }
    if job.leased_message_keys.is_empty()
        && !(job.source_label == "review-feedback"
            && is_terminal_bench_controller_artifact_job(job))
    {
        return false;
    }
    let file_refs = declared_workspace_file_artifacts_for_job(job);
    if file_refs.is_empty() {
        return false;
    }
    let normalized = normalize_token(&format!("{} {} {}", job.goal, job.preview, job.prompt));
    normalized.contains("terminal-bench")
        || normalized.contains("terminal bench")
        || normalized.contains("benchmark")
        || normalized.contains("bench ")
        || normalized.contains("harbor")
        || normalized.contains("controller")
        || normalized.contains("durable artifact")
        || normalized.contains("required durable")
}

fn queue_durable_artifact_timeout_recovery(
    root: &Path,
    job: &QueuedPrompt,
    blocker: &str,
) -> Result<Option<String>> {
    let thread_key = job
        .thread_key
        .clone()
        .unwrap_or_else(|| default_follow_up_thread_key(&job.goal));
    let title = format!("Recover interrupted {}", clip_text(&job.goal, 52));
    let event_key = format!(
        "durable-artifact-timeout-recovery:{}:{}",
        thread_key,
        channels::stable_digest(&title)
    );
    if let Some(existing_title) = existing_timeout_continuation(
        root,
        &thread_key,
        job.workspace_root.as_deref(),
        &job.leased_message_keys,
        &title,
    )? {
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "turn_timeout_continuation",
                conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                severity: "warning",
                reason: "an artifact-backed controller turn hit the runtime time budget",
                action_taken: "reused an existing open durable artifact recovery task",
                details: serde_json::json!({
                    "source_label": job.source_label,
                    "thread_key": thread_key,
                    "title": title,
                    "existing_title": existing_title,
                    "workspace_root": job.workspace_root.clone(),
                    "leased_message_keys": job.leased_message_keys.clone(),
                    "blocker": clip_text(blocker, 180),
                }),
                idempotence_key: Some(&event_key),
            },
        );
        return Ok(Some(format!(
            "existing durable artifact recovery reused: {existing_title}"
        )));
    }

    let created = channels::create_queue_task_with_metadata(
        root,
        channels::QueueTaskCreateRequest {
            title: title.clone(),
            prompt: render_durable_artifact_timeout_recovery_prompt(job, blocker),
            thread_key: thread_key.clone(),
            workspace_root: job.workspace_root.clone(),
            priority: "high".to_string(),
            suggested_skill: job.suggested_skill.clone(),
            parent_message_key: None,
            extra_metadata: Some(serde_json::json!({
                "dedupe_key": event_key,
                "origin_source_label": job.source_label,
                "durable_artifact_timeout_recovery": true,
                "interrupted_message_keys": job.leased_message_keys.clone(),
                "workspace_root": job.workspace_root.clone(),
            })),
        },
    )?;
    let _ = governance::record_event(
        root,
        governance::GovernanceEventRequest {
            mechanism_id: "turn_timeout_continuation",
            conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
            severity: "warning",
            reason: "an artifact-backed controller turn hit the runtime time budget",
            action_taken: "queued a durable artifact recovery task",
            details: serde_json::json!({
                "source_label": job.source_label,
                "thread_key": created.thread_key.clone(),
                "title": created.title.clone(),
                "workspace_root": created.workspace_root.clone(),
                "leased_message_keys": job.leased_message_keys.clone(),
                "blocker": clip_text(blocker, 180),
            }),
            idempotence_key: Some(&format!("{}:queued", created.message_key)),
        },
    );
    Ok(Some(created.title))
}

fn maybe_suppress_fatal_harness_prompt_before_execution(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    job: &QueuedPrompt,
) -> Result<bool> {
    if !is_legacy_timeout_continuation_job(job) {
        return Ok(false);
    }

    let reason =
        "legacy timeout continuation jobs are forbidden because they can recursively restart timed-out harness turns";
    let action =
        "cancelled fatal harness continuation before starting an agent turn; no model tokens spent";
    let details = serde_json::json!({
        "source_label": job.source_label,
        "thread_key": job.thread_key.clone(),
        "ticket_self_work_id": job.ticket_self_work_id.clone(),
        "leased_message_keys": job.leased_message_keys.clone(),
        "leased_ticket_event_keys": job.leased_ticket_event_keys.clone(),
        "goal": clip_text(&job.goal, 180),
        "preview": clip_text(&job.preview, 180),
    });
    let _ = governance::record_event(
        root,
        governance::GovernanceEventRequest {
            mechanism_id: "fatal_harness_loop_guard",
            conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
            severity: "critical",
            reason,
            action_taken: action,
            details,
            idempotence_key: Some(&format!(
                "fatal-harness-loop-guard:{}:{}",
                job.thread_key
                    .as_deref()
                    .unwrap_or(job.source_label.as_str()),
                job.leased_message_keys
                    .first()
                    .map(String::as_str)
                    .or(job.ticket_self_work_id.as_deref())
                    .unwrap_or(job.goal.as_str())
            )),
        },
    );

    if !job.leased_message_keys.is_empty() {
        channels::ack_leased_messages(root, &job.leased_message_keys, "cancelled")?;
    }
    if !job.leased_ticket_event_keys.is_empty() {
        let _ = tickets::ack_leased_ticket_events(root, &job.leased_ticket_event_keys, "failed");
    }
    if let Some(work_id) = job.ticket_self_work_id.as_deref() {
        block_ticket_self_work_item(root, work_id, reason);
    }

    let mut shared = lock_shared_state(state);
    release_leased_keys_locked(
        &mut shared,
        &job.leased_message_keys,
        &job.leased_ticket_event_keys,
    );
    shared.busy = false;
    shared.current_goal_preview = None;
    shared.active_source_label = None;
    shared.last_error = Some("fatal harness timeout continuation suppressed".to_string());
    shared.last_progress_epoch_secs = current_epoch_secs();
    push_event_locked(
        &mut shared,
        format!(
            "Suppressed fatal harness continuation before model execution: {}",
            clip_text(&job.goal, 120)
        ),
    );

    Ok(true)
}

fn is_legacy_timeout_continuation_job(job: &QueuedPrompt) -> bool {
    let prompt = normalize_token(&job.prompt);
    let goal = normalize_token(&job.goal);
    let preview = normalize_token(&job.preview);

    prompt.contains("art: timeout-continuation")
        || prompt.contains("durable continuation:")
        || (goal.starts_with("continue ")
            && goal.contains(" after timeout")
            && (prompt.contains("runtime stop:") || prompt.contains("direct session timeout")))
        || (preview.starts_with("continue ")
            && preview.contains(" after timeout")
            && (prompt.contains("runtime stop:") || prompt.contains("direct session timeout")))
}

fn maybe_enqueue_runtime_retry_continuation(
    root: &Path,
    job: &QueuedPrompt,
    error_text: &str,
) -> Result<Option<String>> {
    if !runtime_error_is_transient_api_failure(error_text) {
        return Ok(None);
    }
    if job.source_label == "review-feedback" && is_terminal_bench_preflight_violation(error_text) {
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "terminal_bench_review_feedback_guard",
                conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                severity: "warning",
                reason: "review-feedback hit the first-turn Terminal-Bench preflight guard",
                action_taken:
                    "suppressed durable runtime retry self-work; the original controller/queue state must carry the feedback",
                details: serde_json::json!({
                    "source_label": job.source_label,
                    "thread_key": job.thread_key.clone(),
                    "workspace_root": job.workspace_root.clone(),
                    "error": clip_text(error_text, 220),
                }),
                idempotence_key: Some(&format!(
                    "tb2-review-feedback-preflight:{}",
                    job.thread_key.as_deref().unwrap_or(job.goal.as_str())
                )),
            },
        );
        return Ok(None);
    }
    let thread_key = job
        .thread_key
        .clone()
        .unwrap_or_else(|| default_follow_up_thread_key(&job.goal));
    let title = format!("Retry {} after API failure", clip_text(&job.goal, 52));
    let event_key = format!("runtime-api-retry:{thread_key}:{title}");
    let not_before = runtime_retry_not_before_iso(error_text);
    if !job.leased_message_keys.is_empty() || job.ticket_self_work_id.is_some() {
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "runtime_api_retry_continuation",
                conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                severity: "warning",
                reason: "the previous agent run hit a retryable model API failure",
                action_taken:
                    "kept the existing durable work item open with retry feedback/cooldown; no new retry task queued",
                details: serde_json::json!({
                    "source_label": job.source_label,
                    "thread_key": thread_key,
                    "title": title,
                    "error": clip_text(error_text, 220),
                    "has_outbound_email": job.outbound_email.is_some(),
                    "outbound_anchor": job.outbound_anchor.clone(),
                    "not_before": not_before,
                }),
                idempotence_key: Some(&event_key),
            },
        );
        return Ok(None);
    }

    let _ = governance::record_event(
        root,
        governance::GovernanceEventRequest {
            mechanism_id: "runtime_api_retry_continuation",
            conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
            severity: "critical",
            reason: "the previous agent run hit a retryable model API failure",
            action_taken:
                "suppressed standalone runtime retry task; the operator or original durable work must resume after cooldown",
            details: serde_json::json!({
                "source_label": job.source_label,
                "thread_key": thread_key,
                "title": title,
                "error": clip_text(error_text, 220),
                "has_outbound_email": job.outbound_email.is_some(),
                "outbound_anchor": job.outbound_anchor.clone(),
                "not_before": not_before,
            }),
            idempotence_key: Some(&event_key),
        },
    );
    Ok(None)
}

fn apply_runtime_retry_feedback_to_leased_queue(
    root: &Path,
    job: &QueuedPrompt,
    error_text: &str,
) -> Result<usize> {
    if !runtime_error_is_transient_api_failure(error_text) {
        return Ok(0);
    }
    let feedback_prompt = render_runtime_retry_prompt(job, error_text);
    let note = format!(
        "Harness retry feedback injected after runtime failure: {}",
        clip_text(error_text.trim(), 180)
    );
    let mut updated = 0usize;
    for message_key in &job.leased_message_keys {
        channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: message_key.clone(),
                prompt: Some(feedback_prompt.clone()),
                workspace_root: job.workspace_root.clone(),
                status_note: Some(note.clone()),
                ..Default::default()
            },
        )?;
        updated += 1;
    }
    Ok(updated)
}

fn runtime_retry_not_before_iso(error_text: &str) -> String {
    let cooldown_secs = turn_loop::hard_runtime_blocker_retry_cooldown_secs(error_text)
        .unwrap_or(300)
        .clamp(30, 1_800);
    chrono_like_iso(current_epoch_secs().saturating_add(cooldown_secs))
}

fn timeout_retry_not_before_iso(agent_failure_count: i64) -> String {
    let exponent = agent_failure_count.saturating_sub(1).clamp(0, 4) as u32;
    let cooldown_secs = 300_u64.saturating_mul(2_u64.saturating_pow(exponent));
    chrono_like_iso(current_epoch_secs().saturating_add(cooldown_secs.min(3_600)))
}

fn is_turn_timeout_blocker(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.contains("timed out after")
        || lowered.contains("time budget")
        || lowered.contains("session timeout")
}

fn is_compaction_blocker(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.contains("mid-task compaction timeout")
        || lowered.contains("compaction timeout")
        || lowered.contains("compact_followup")
}

fn is_no_assistant_message_blocker(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.contains("turn completed without assistant message")
        || lowered.contains("completed without assistant message")
        || lowered.contains("no assistant message")
        || lowered.contains("empty assistant message")
}

fn is_terminal_bench_preflight_violation(value: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains("terminal-bench preflight violation")
}

fn terminal_bench_preflight_retry_loop_should_stop(job: &QueuedPrompt, error_text: &str) -> bool {
    if !is_terminal_bench_preflight_violation(error_text) {
        return false;
    }
    job.prompt
        .to_ascii_lowercase()
        .contains("harness terminal-bench preflight retry")
}

fn terminal_bench_preflight_loop_stop_note(error_text: &str) -> String {
    format!(
        "Stopped automatic Terminal-Bench preflight retry loop. The worker repeated the preflight violation after explicit harness feedback; the model must be restarted with a clearer controller prompt or a stronger model. Last error: {}",
        clip_text(error_text.trim(), 220)
    )
}

/// F3: classify a harness-error string into a structured `AgentOutcome`.
/// The error text comes from the harness/turn-loop itself (we own its
/// format), not from free-form prompt content. Keep the matchers narrow
/// and stable; downstream branching always reads the structured value.
pub(crate) fn classify_agent_failure(error_text: &str) -> lcm::AgentOutcome {
    if is_turn_timeout_blocker(error_text) {
        return lcm::AgentOutcome::TurnTimeout;
    }
    if is_compaction_blocker(error_text) {
        return lcm::AgentOutcome::Aborted;
    }
    let lowered = error_text.to_ascii_lowercase();
    if lowered.contains("cancelled") || lowered.contains("canceled") {
        return lcm::AgentOutcome::Cancelled;
    }
    if lowered.contains("aborted") || lowered.contains("invariant violated") {
        return lcm::AgentOutcome::Aborted;
    }
    lcm::AgentOutcome::ExecutionError
}

fn render_timeout_continue_prompt(
    goal: &str,
    blocker: &str,
    workspace_root: Option<&str>,
) -> String {
    let summarized_goal = summarize_follow_up_goal(goal);
    let prompt = format!(
        "HARNESS FEEDBACK\nProblem: The previous turn stopped before the task reached a verified finish.\n\nCURRENT TASK\n{}\n\nSTOP REASON\n{}\n\nREQUIRED ACTIONS\n- Re-check durable runtime state, queue state, progress artifacts, and repository/runtime state before continuing.\n- Preserve work that already exists; do not restart from scratch unless state proves it is necessary.\n- Continue with the next smallest concrete step.\n- If more than one turn is still needed, leave exactly one open CTOX plan, queue item, self-work item, follow-up, or schedule before this turn ends.\n- A sentence in the reply does not count as open work.\n- Ask the owner only if the real blocker is external.\n\nEXIT GATE\nFinish only after the real durable outcome exists, or after exact next work has been persisted in CTOX runtime state.",
        summarized_goal,
        clip_text(blocker.trim(), 220)
    );
    prepend_workspace_contract(&prompt, workspace_root)
}

fn render_durable_artifact_timeout_recovery_prompt(job: &QueuedPrompt, blocker: &str) -> String {
    let file_refs = declared_workspace_file_artifacts_for_job(job);
    let mut prompt = format!(
        "HARNESS FEEDBACK\nProblem: The previous slice reached its runtime budget before the controller reached a terminal durable outcome. This is not completion.\n\nCURRENT TASK\n{}\n\nSTOP REASON\n{}\n\nDURABLE FILES THAT MUST STAY UPDATED\n",
        summarize_follow_up_goal(&job.goal),
        clip_text(blocker.trim(), 220)
    );
    for path in &file_refs {
        prompt.push_str("- ");
        prompt.push_str(path);
        prompt.push('\n');
    }
    prompt.push_str(
        "\nREQUIRED ACTIONS\n- Inspect the workspace and the listed files first; preserve valid progress.\n- Continue the same controller run from the durable files instead of restarting from scratch.\n- Each listed path must be a regular file. A directory at a required file path is invalid and must be corrected before any completion claim.\n- Keep the logbook and summary truthful about attempted work, discovered tasks, blockers, and next actions.\n- If benchmark execution still needs another slice, persist exactly one concrete queue item or plan item before ending.\n\nEXIT GATE\nFinish only after the durable outcome exists in the listed files and the benchmark controller is either terminal or has exactly one persisted next action.",
    );
    if is_terminal_bench_controller_artifact_job(job) {
        let basenames = file_refs
            .iter()
            .filter_map(|path| {
                Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        let results_name = basenames
            .iter()
            .find(|name| name.starts_with("results."))
            .cloned()
            .unwrap_or_else(|| "results.jsonl".to_string());
        let log_name = basenames
            .iter()
            .find(|name| *name == "logbook.md" || *name == "run-log.md")
            .cloned()
            .unwrap_or_else(|| "logbook.md".to_string());
        prompt.push_str(&format!(
            "\n\nTERMINAL-BENCH TIMEOUT RECOVERY ORDER\n1. Read the listed files and recent context only far enough to recover the current phase.\n2. Immediately write a checkpoint into controller.json, {log_name}, knowledge.md, and {results_name} before any further discovery.\n3. The checkpoint must include verified runtime facts, discovered task source, runner/container blocker status, no-solutions policy status, and the exact next action.\n4. After every further tool call that discovers facts or changes benchmark state, the next tool call must persist those facts into the durable files.\n5. Do not finish with a prose summary only. The durable files are the state."
        ));
    }
    prepend_workspace_contract(&prompt, job.workspace_root.as_deref())
}

fn render_runtime_retry_prompt(job: &QueuedPrompt, error_text: &str) -> String {
    let terminal_bench_preflight_retry = is_terminal_bench_preflight_violation(error_text);
    let mut required_actions = vec![
        "inspect durable state and workspace artifacts before retrying; do not trust the previous reply text as proof",
        "preserve work that already exists and avoid duplicate queue tasks",
        "retry only the smallest step interrupted by the runtime or harness failure",
        "finish only after the real durable outcome exists in the state machine",
        "if the runtime is still unavailable, leave this work pending for another retry instead of claiming completion",
    ];
    let problem = if terminal_bench_preflight_retry {
        required_actions.insert(
            0,
            "the previous first shell command violated the Terminal-Bench preflight gate; the next worker action must be a shell script that creates the current RUN_DIR, creates all required files as regular files, records real queue refs or an exact CLI blocker, and verifies every file with test -f before any discovery",
        );
        required_actions.insert(
            1,
            "do not inspect install trees, old run directories, Harbor, datasets, web pages, GPUs, or runtime state until the current RUN_DIR artifacts exist and the blocker or queue refs are persisted",
        );
        "The previous worker turn started with the wrong shell action for a Terminal-Bench controller preflight. The task is not complete; this is actionable harness feedback, not work performed by the harness."
    } else if is_no_assistant_message_blocker(error_text) {
        required_actions.insert(
            0,
            "the previous model turn executed at least one tool phase but ended without a final assistant message; continue after the tool phase instead of restarting blindly",
        );
        "The previous model turn ended after runtime/tool work without producing the required final assistant message. The task is not complete yet."
    } else {
        "The previous turn was interrupted by a retryable runtime failure. The task is not complete yet."
    };
    if job.outbound_email.is_some() {
        required_actions.push(
            "for a proactive outbound email task, produce the final send-ready body first; after review approval, continue only from the exact reviewed-send continuation prompt",
        );
        required_actions.push(
            "do not say the email was sent unless an outbound email row reached the accepted terminal state",
        );
    }
    let mut prompt = format!(
        "HARNESS FEEDBACK\nProblem: {problem}\n\nCURRENT TASK\n{}\n\nRUNTIME FAILURE\n{}\n\nREQUIRED ACTIONS\n- {}\n\nEXIT GATE\nFinish only after the durable outcome exists in runtime state. If the runtime is still unavailable, keep the work pending instead of claiming completion.",
        runtime_retry_current_task_summary(job),
        clip_text(error_text.trim(), 220),
        required_actions.join("\n- ")
    );
    if terminal_bench_preflight_retry {
        let file_refs = declared_workspace_file_artifacts_for_job(job);
        if !file_refs.is_empty() {
            prompt.push_str(
                "\n\nHARNESS TERMINAL-BENCH PREFLIGHT RETRY\nOnly required durable files for this controller turn:\n",
            );
            for path in &file_refs {
                prompt.push_str("- ");
                prompt.push_str(path);
                prompt.push('\n');
            }
            prompt.push_str(
                "The controller must create preparation queue/tickets and record real queue:system::* keys, or persist an exact blocker with the failed CLI command and stderr. Do not satisfy this retry with substitute files outside this list.\n",
            );
        }
    }
    prepend_workspace_contract(&prompt, job.workspace_root.as_deref())
}

fn runtime_retry_current_task_summary(job: &QueuedPrompt) -> String {
    let prompt = strip_harness_feedback_wrappers(&job.prompt);
    let goal = strip_harness_feedback_wrappers(&job.goal);
    let candidate = if !prompt.trim().is_empty() && prompt.trim() != job.prompt.trim() {
        prompt
    } else {
        goal
    };
    summarize_follow_up_goal(candidate)
}

fn strip_harness_feedback_wrappers(value: &str) -> &str {
    let mut current = value.trim();
    loop {
        let Some(current_task_start) = current.find("\n\nCURRENT TASK\n") else {
            return current;
        };
        let after_current_task = current_task_start + "\n\nCURRENT TASK\n".len();
        let Some(runtime_failure_start) =
            current[after_current_task..].find("\n\nRUNTIME FAILURE\n")
        else {
            return current;
        };
        current = current[after_current_task..after_current_task + runtime_failure_start].trim();
    }
}

fn runnable_thread_task_exists_ignoring(
    root: &Path,
    thread_key: &str,
    ignored_message_keys: &[String],
) -> Result<bool> {
    let tasks =
        channels::list_queue_tasks(root, &["pending".to_string(), "leased".to_string()], 64)?;
    Ok(tasks.into_iter().any(|task| {
        task.thread_key == thread_key
            && !ignored_message_keys
                .iter()
                .any(|key| key == &task.message_key)
    }))
}

fn summarize_follow_up_goal(goal: &str) -> String {
    let trimmed = goal.trim();
    if trimmed.is_empty() {
        return "reconstruct the next concrete slice from durable continuity".to_string();
    }

    for marker in ["Goal:\n", "Mission:\n", "Slice goal:\n"] {
        if let Some(start) = trimmed.find(marker) {
            let remainder = &trimmed[start + marker.len()..];
            let block = remainder
                .split("\n\n")
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(trimmed);
            return clip_text(block, 320);
        }
    }

    clip_text(trimmed, 320)
}

fn fallback_text<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    }
}

fn runtime_blocker_backoff_remaining_secs(shared: &SharedState) -> Option<u64> {
    let error = shared.last_error.as_deref()?;
    let cooldown_secs = turn_loop::hard_runtime_blocker_retry_cooldown_secs(error)?;
    let elapsed_secs = current_epoch_secs().saturating_sub(shared.last_progress_epoch_secs);
    if elapsed_secs < cooldown_secs {
        Some(cooldown_secs - elapsed_secs)
    } else {
        None
    }
}

fn existing_timeout_continuation(
    root: &Path,
    thread_key: &str,
    workspace_root: Option<&str>,
    leased_message_keys: &[String],
    title: &str,
) -> Result<Option<String>> {
    let tasks = channels::list_queue_tasks(
        root,
        &[
            "pending".to_string(),
            "leased".to_string(),
            "blocked".to_string(),
        ],
        64,
    )?;
    let normalized_title = normalize_token(title);
    if let Some(existing) = tasks.iter().find(|task| {
        task.thread_key == thread_key
            && !leased_message_keys
                .iter()
                .any(|key| key == &task.message_key)
            && normalize_token(&task.title) == normalized_title
    }) {
        return Ok(Some(existing.title.clone()));
    }
    let normalized_workspace_root = workspace_root
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(expected_workspace_root) = normalized_workspace_root {
        let workspace_matches = tasks
            .iter()
            .filter(|task| {
                matches!(task.route_status.as_str(), "pending" | "leased")
                    && !leased_message_keys
                        .iter()
                        .any(|key| key == &task.message_key)
                    && task.workspace_root.as_deref() == Some(expected_workspace_root)
            })
            .collect::<Vec<_>>();
        if workspace_matches.len() == 1 {
            return Ok(Some(workspace_matches[0].title.clone()));
        }
    }
    Ok(tasks
        .into_iter()
        .find(|task| {
            task.thread_key == thread_key
                && matches!(task.route_status.as_str(), "pending" | "leased")
                && !leased_message_keys
                    .iter()
                    .any(|key| key == &task.message_key)
        })
        .map(|task| task.title))
}

fn assess_current_context_health(
    root: &Path,
    db_path: &Path,
    conversation_id: i64,
    latest_prompt: Option<&str>,
) -> Option<context_health::ContextHealthSnapshot> {
    let max_context = runtime_kernel::InferenceRuntimeKernel::resolve(root)
        .ok()
        .map(|runtime| runtime.turn_context_tokens())
        .unwrap_or(131_072);
    context_health::assess_for_conversation(db_path, conversation_id, max_context, latest_prompt)
        .ok()
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

fn default_follow_up_thread_key(goal: &str) -> String {
    let digest = {
        use sha2::Digest;
        let bytes = sha2::Sha256::digest(goal.as_bytes());
        let hex = format!("{bytes:x}");
        hex[..12].to_string()
    };
    format!("queue/follow-up-{digest}")
}

fn build_queue_guard_prompt(root: &Path, pending: usize) -> String {
    let ctox_bin = preferred_ctox_executable(root)
        .unwrap_or_else(|_| std::env::current_exe().unwrap_or_else(|_| root.join("ctox")));
    format!(
        "Use the queue-cleanup skill first. The CTOX service queue is under pressure with {pending} queued prompt(s). Before doing any normal work, inspect the service state for this root: {}. Prefer the local CLI binary `{}` with `status`, `schedule list`, and `queue list`. If that binary is unavailable, inspect `runtime/ctox_service.log` plus the runtime databases directly instead of assuming `ctox` is on PATH. Find the source of repeated or flooding work, pause or contain any schedule that is filling the queue, avoid duplicate follow-up tasks, and keep only the minimum safe next work moving. Use `ctox queue spill-candidates` to identify explicit spillover candidates, `ctox queue spill --message-key <key>` to park valid work in the internal ticket system, `ctox queue spills` to review parked work, and `ctox queue restore --message-key <key>` to rehydrate it later. Treat queue recovery as top priority and report what was paused, deduplicated, blocked, spilled, restored, or left active.",
        root.display(),
        ctox_bin.display()
    )
}

#[derive(Debug, Clone)]
struct SystemdUnitStatus {
    active: bool,
    enabled: bool,
    pid: Option<u32>,
}

fn systemd_unit_status(root: &Path) -> Result<Option<SystemdUnitStatus>> {
    if !systemd_user_available() || !systemd_user_unit_installed(root) {
        return Ok(None);
    }
    let active = match systemctl_user(["is-active", "--quiet", SYSTEMD_USER_UNIT_NAME]) {
        Ok(()) => true,
        Err(_) => false,
    };
    let enabled_output = systemctl_user_capture(["is-enabled", SYSTEMD_USER_UNIT_NAME])?;
    let enabled_stdout = String::from_utf8_lossy(&enabled_output.stdout)
        .trim()
        .to_string();
    let enabled = enabled_output.status.success()
        && matches!(
            enabled_stdout.as_str(),
            "enabled" | "enabled-runtime" | "static"
        );
    let pid_output = systemctl_user_capture([
        "show",
        SYSTEMD_USER_UNIT_NAME,
        "--property",
        "MainPID",
        "--value",
    ])?;
    let pid = if pid_output.status.success() {
        String::from_utf8_lossy(&pid_output.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
    } else {
        None
    };
    Ok(Some(SystemdUnitStatus {
        active,
        enabled,
        pid,
    }))
}

fn systemd_user_available() -> bool {
    cfg!(target_os = "linux")
        && Command::new("systemctl")
            .arg("--user")
            .arg("--version")
            .output()
            .is_ok()
}

fn systemd_user_unit_installed(root: &Path) -> bool {
    if root.join("runtime/ctox_systemd_user.installed").exists() {
        return true;
    }
    let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        });
    let Some(config_home) = xdg_config_home else {
        return false;
    };
    let unit_path = config_home
        .join("systemd/user")
        .join(SYSTEMD_USER_UNIT_NAME);
    if !unit_path.exists() {
        return false;
    }
    let Ok(unit_text) = std::fs::read_to_string(&unit_path) else {
        return false;
    };
    let normalized_root = root.display().to_string();
    let working_directory = format!("WorkingDirectory={normalized_root}");
    let ctox_root_env = format!("Environment=CTOX_ROOT={normalized_root}");
    unit_text
        .lines()
        .map(str::trim)
        .any(|line| line == working_directory || line == ctox_root_env)
}

fn systemctl_user<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let output = systemctl_user_capture(args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = if !stderr.is_empty() { stderr } else { stdout };
    anyhow::bail!("systemctl --user failed: {message}");
}

fn systemctl_user_capture<I, S>(args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut command = Command::new("systemctl");
    command.arg("--user");
    configure_systemctl_user_env(&mut command);
    let mut rendered_args = vec!["--user".to_string()];
    for arg in args {
        rendered_args.push(arg.as_ref().to_string());
        command.arg(arg.as_ref());
    }
    command_output_with_timeout(
        &mut command,
        Duration::from_secs(SYSTEMCTL_USER_TIMEOUT_SECS),
        &format!("systemctl {}", rendered_args.join(" ")),
    )
}

fn command_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
    description: &str,
) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to launch {description}"))?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .with_context(|| format!("failed to poll {description}"))?
            .is_some()
        {
            return child
                .wait_with_output()
                .with_context(|| format!("failed to collect {description} output"));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let reap_deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < reap_deadline {
                if child
                    .try_wait()
                    .with_context(|| format!("failed to poll {description}"))?
                    .is_some()
                {
                    return child
                        .wait_with_output()
                        .with_context(|| format!("failed to collect {description} output"));
                }
                thread::sleep(Duration::from_millis(50));
            }
            anyhow::bail!("{description} timed out after {}s", timeout.as_secs());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn configure_systemctl_user_env(command: &mut Command) {
    #[cfg(unix)]
    {
        let runtime_dir = std::path::PathBuf::from(format!("/run/user/{}", unsafe { geteuid() }));
        if runtime_dir.is_dir() {
            command.env("XDG_RUNTIME_DIR", &runtime_dir);
            let bus_path = runtime_dir.join("bus");
            if bus_path.exists() {
                command.env(
                    "DBUS_SESSION_BUS_ADDRESS",
                    format!("unix:path={}", bus_path.display()),
                );
            }
        }
    }
}

fn now_iso_string() -> String {
    chrono_like_iso(current_epoch_secs())
}

fn current_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn chrono_like_iso(epoch_seconds: u64) -> String {
    use std::fmt::Write as _;

    let seconds_per_day = 86_400u64;
    let days = epoch_seconds / seconds_per_day;
    let seconds_of_day = epoch_seconds % seconds_per_day;

    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };

    let mut output = String::with_capacity(20);
    let _ = write!(
        output,
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    );
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lcm::{ContinuityKind, LcmConfig, LcmEngine};
    use crate::plan;
    use crate::secrets;
    use serde_json::json;

    fn temp_root(prefix: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ctox-service-{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn review_outcome_for_no_send_test(summary: &str) -> review::ReviewOutcome {
        let mut outcome = review::ReviewOutcome::skipped(summary);
        outcome.required = true;
        outcome.verdict = review::ReviewVerdict::Fail;
        outcome.score = 25;
        outcome
    }

    #[test]
    fn review_no_send_wait_is_terminal() {
        let mut outcome = review_outcome_for_no_send_test(
            "Do not send a founder reply yet. The CRM thread is in wait mode until Marco provides the CRM/tool and sync scope.",
        );
        outcome.failed_gates.push(
            "No-send: wait until the founders provide concrete technical inputs.".to_string(),
        );
        outcome.evidence.push(
            "Michael's latest thread says CTO1 should support technically after the decision."
                .to_string(),
        );

        assert!(review_outcome_is_terminal_no_send(&outcome));
    }

    #[test]
    fn review_missing_founder_work_is_not_terminal_no_send() {
        let mut outcome = review_outcome_for_no_send_test(
            "Do not send the current mail because missing deliverables must be done before contacting the founders.",
        );
        outcome.failed_gates.push(
            "Missing required dashboard link and evidence; send a corrected reply after rework."
                .to_string(),
        );

        assert!(!review_outcome_is_terminal_no_send(&outcome));
    }

    fn upsert_test_inbound_message(
        root: &Path,
        message_key: &str,
        channel: &str,
        thread_key: &str,
        sender_address: &str,
        subject: &str,
        body: &str,
        metadata: Value,
    ) {
        let db_path = crate::paths::core_db(&root);
        let mut conn = channels::open_channel_db(&db_path).expect("open channel db");
        let observed_at = "2026-04-28T12:00:00Z";
        channels::upsert_communication_message(
            &mut conn,
            channels::UpsertMessage {
                message_key,
                channel,
                account_key: &format!("{channel}:test"),
                thread_key,
                remote_id: message_key,
                direction: "inbound",
                folder_hint: "inbox",
                sender_display: "Test Sender",
                sender_address,
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject,
                preview: &body[..body.len().min(120)],
                body_text: body,
                body_html: "",
                raw_payload_ref: "",
                trust_level: "internal",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: observed_at,
                observed_at,
                metadata_json: &serde_json::to_string(&metadata).expect("metadata json"),
            },
        )
        .expect("upsert message");
        channels::refresh_thread(&mut conn, thread_key).expect("refresh thread");
        channels::ensure_routing_rows_for_inbound(&conn).expect("routing rows");
    }

    fn route_status_for(root: &Path, message_key: &str) -> String {
        let conn =
            channels::open_channel_db(&crate::paths::core_db(&root)).expect("open channel db");
        conn.query_row(
            "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
            params![message_key],
            |row| row.get(0),
        )
        .expect("route status")
    }

    fn routed_email_message(sender_address: &str) -> channels::RoutedInboundMessage {
        channels::RoutedInboundMessage {
            message_key: "m1".to_string(),
            channel: "email".to_string(),
            account_key: "email:test".to_string(),
            thread_key: "email-thread".to_string(),
            sender_display: "Sender".to_string(),
            sender_address: sender_address.to_string(),
            subject: "Meeting".to_string(),
            preview: "Meeting".to_string(),
            body_text: "Join https://meet.google.com/abc-defg-hij".to_string(),
            external_created_at: "2026-04-28T12:00:00Z".to_string(),
            workspace_root: None,
            metadata: json!({}),
            preferred_reply_modality: None,
        }
    }

    fn routed_teams_message() -> channels::RoutedInboundMessage {
        channels::RoutedInboundMessage {
            message_key: "teams-msg-1".to_string(),
            channel: "teams".to_string(),
            account_key: "teams:bot".to_string(),
            thread_key: "teams:bot::chat::chat-123".to_string(),
            sender_display: "Alice".to_string(),
            sender_address: "user-alice".to_string(),
            subject: String::new(),
            preview: "Bitte pruefen".to_string(),
            body_text: "Bitte pruefen".to_string(),
            external_created_at: "2026-04-28T12:00:00Z".to_string(),
            workspace_root: None,
            metadata: json!({"teams_chat_id": "chat-123"}),
            preferred_reply_modality: None,
        }
    }

    #[test]
    fn web_extraction_teams_message_suggests_universal_scraping_skill() {
        let mut message = routed_teams_message();
        message.preview = "Aussteller aus Webseite in Excel uebertragen".to_string();
        message.body_text = "https://www.intersolar.de/ausstellerliste\nDie Webseite laedt erst nach wenn man scrollt. Bitte lese alle Aussteller aus Deutschland aus und uebertrage diese in eine Excel.".to_string();

        assert_eq!(
            suggested_skill_from_message(&message).as_deref(),
            Some("universal-scraping")
        );
    }

    #[test]
    fn explicit_message_skill_metadata_overrides_web_extraction_inference() {
        let mut message = routed_teams_message();
        message.body_text =
            "https://example.com bitte alle Eintraege auslesen und in Excel uebertragen."
                .to_string();
        message.metadata = json!({"skill": "owner-communication"});

        assert_eq!(
            suggested_skill_from_message(&message).as_deref(),
            Some("owner-communication")
        );
    }

    #[test]
    fn queue_guard_inserts_once_at_front_when_threshold_reached() {
        let root = temp_root("queue-guard");
        let mut shared = SharedState::default();
        shared.pending_prompts = (0..QUEUE_PRESSURE_GUARD_THRESHOLD)
            .map(|index| QueuedPrompt {
                prompt: format!("prompt-{index}"),
                goal: format!("goal-{index}"),
                preview: format!("preview-{index}"),
                source_label: "cron".to_string(),
                suggested_skill: None,
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: Vec::new(),
                thread_key: None,
                workspace_root: None,
                ticket_self_work_id: None,
                outbound_email: None,
                outbound_anchor: None,
            })
            .collect();

        ensure_queue_guard_locked(&root, &mut shared);
        ensure_queue_guard_locked(&root, &mut shared);

        assert_eq!(
            shared
                .pending_prompts
                .front()
                .map(|item| item.source_label.as_str()),
            Some(QUEUE_GUARD_SOURCE_LABEL)
        );
        assert_eq!(
            shared
                .pending_prompts
                .iter()
                .filter(|item| item.source_label == QUEUE_GUARD_SOURCE_LABEL)
                .count(),
            1
        );
        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events
            .iter()
            .any(|event| event.mechanism_id == "queue_pressure_guard"));
    }

    #[test]
    fn boot_state_invariant_check_records_visible_violation_event() {
        let root = temp_root("boot-state-invariants");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let db_path = crate::paths::core_db(&root);
        let engine = LcmEngine::open(&db_path, LcmConfig::default()).unwrap();
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap();
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                ContinuityKind::Focus,
                "## Status\n+ Mission: Legacy split-brain closure state.\n+ Mission state: done.\n+ Continuation mode: closed.\n+ Trigger intensity: cold.\n## Next\n+ Next slice: none.\n## Done / Gate\n+ Done gate: stale closure.\n+ Closure confidence: complete.\n",
            )
            .unwrap();
        plan::handle_plan_command(
            &root,
            &[
                "ingest".to_string(),
                "--title".to_string(),
                "canonical split brain continuation".to_string(),
                "--prompt".to_string(),
                "Reopen the canonical mission from split-brain state.".to_string(),
            ],
        )
        .unwrap();

        let state = Arc::new(Mutex::new(SharedState::default()));
        run_boot_state_invariant_check(&root, &state);

        let recent_events = {
            let shared = lock_shared_state(&state);
            shared.recent_events.iter().cloned().collect::<Vec<_>>()
        };
        assert!(recent_events
            .iter()
            .any(|event| event.contains("State invariants at boot")));
        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events.iter().any(|event| {
            event.mechanism_id == "state_invariant_guard"
                && event.reason == "boot_state_invariants_violation"
        }));
    }

    #[test]
    fn boot_state_invariant_check_repairs_partial_commit_focus_conflict() {
        let root = temp_root("boot-state-invariants-repair");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let db_path = crate::paths::core_db(&root);
        let engine = LcmEngine::open(&db_path, LcmConfig::default()).unwrap();
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap();
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                ContinuityKind::Focus,
                "## Status\n+ Mission: Old continuity head before partial-commit recovery.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: warm.\n## Blocker\n+ Current blocker: the recovery path still points at the old continuity head.\n## Next\n+ Next slice: advance to the new continuity head.\n## Done / Gate\n+ Done gate: resync the live mission state to the newest continuity head.\n+ Closure confidence: low.\n",
            )
            .unwrap();
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                ContinuityKind::Focus,
                "## Status\n+ Mission: Keep the newest continuity head primary after partial-commit recovery.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker: the live mission cache may still point at the old focus head.\n## Next\n+ Next slice: verify the newest focus head is the active runtime truth.\n## Done / Gate\n+ Done gate: keep the newest focus head primary and leave exactly one bounded continuation open.\n",
            )
            .unwrap();

        let state = Arc::new(Mutex::new(SharedState::default()));
        run_boot_state_invariant_check(&root, &state);

        let report = state_invariants::evaluate_runtime_state_invariants(
            &root,
            turn_loop::CHAT_CONVERSATION_ID,
        )
        .expect("failed to evaluate invariants after repair");
        assert!(report.is_clean());
        assert_eq!(
            report.mission_state.mission,
            "Keep the newest continuity head primary after partial-commit recovery."
        );

        let recent_events = {
            let shared = lock_shared_state(&state);
            shared.recent_events.iter().cloned().collect::<Vec<_>>()
        };
        assert!(recent_events
            .iter()
            .any(|event| event.contains("State invariants repaired at boot")));
        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events.iter().any(|event| {
            event.mechanism_id == "state_invariant_guard"
                && event.reason == "boot_state_invariants_repaired"
        }));
    }

    #[test]
    fn boot_state_invariant_check_reopens_mission_when_runtime_work_is_still_open() {
        let root = temp_root("boot-state-runtime-open");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let db_path = crate::paths::core_db(&root);
        let engine = LcmEngine::open(&db_path, LcmConfig::default()).unwrap();
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap();
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                ContinuityKind::Focus,
                "## Status\n+ Mission: Keep the newest continuity head primary after partial-commit recovery.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker: the live mission cache may still point at the old focus head.\n## Next\n+ Next slice: verify the newest focus head is the active runtime truth.\n## Done / Gate\n+ Done gate: keep the newest focus head primary and leave exactly one bounded continuation open.\n+ Closure confidence: low.\n",
            )
            .unwrap();
        plan::handle_plan_command(
            &root,
            &[
                "ingest".to_string(),
                "--title".to_string(),
                "partial commit resync restart verification".to_string(),
                "--prompt".to_string(),
                "After restart, verify the newest focus head is still authoritative.".to_string(),
            ],
        )
        .unwrap();

        let current = engine
            .stored_mission_state(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap()
            .expect("missing stored mission state");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE mission_states
             SET mission_status = ?1, continuation_mode = ?2, closure_confidence = ?3,
                 is_open = ?4, allow_idle = ?5, focus_head_commit_id = ?6
             WHERE conversation_id = ?7",
            rusqlite::params![
                "done",
                "continuous",
                "high",
                0,
                1,
                current.focus_head_commit_id,
                turn_loop::CHAT_CONVERSATION_ID,
            ],
        )
        .unwrap();
        drop(conn);

        let state = Arc::new(Mutex::new(SharedState::default()));
        run_boot_state_invariant_check(&root, &state);

        let report = state_invariants::evaluate_runtime_state_invariants(
            &root,
            turn_loop::CHAT_CONVERSATION_ID,
        )
        .expect("failed to evaluate invariants after open-work repair");
        assert!(
            report.is_clean(),
            "unexpected violations: {:?}",
            report.violations
        );
        assert_eq!(report.mission_state.mission_status, "active");
        assert!(report.mission_state.is_open);
        assert!(!report.mission_state.allow_idle);

        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events.iter().any(|event| {
            event.mechanism_id == "state_invariant_guard"
                && event.reason == "boot_state_invariants_repaired"
                && event.action_taken == "reopened_mission_state_for_open_runtime_work"
        }));
    }

    #[test]
    fn turn_end_state_invariant_check_reopens_mission_when_runtime_work_is_still_open() {
        let root = temp_root("turn-state-runtime-open");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let db_path = crate::paths::core_db(&root);
        let engine = LcmEngine::open(&db_path, LcmConfig::default()).unwrap();
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap();
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                ContinuityKind::Focus,
                "## Status\n+ Mission: Reopen runtime work after turn-end invariant repair.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker: the mission cache may have drifted closed while plan work is still open.\n## Next\n+ Next slice: rehydrate the newest focus truth after the worker turn.\n## Done / Gate\n+ Done gate: keep exactly one bounded continuation open until the runtime work is closed.\n+ Closure confidence: low.\n",
            )
            .unwrap();
        plan::handle_plan_command(
            &root,
            &[
                "ingest".to_string(),
                "--title".to_string(),
                "Verify stale mission continuity truth after rehydrate restart".to_string(),
                "--prompt".to_string(),
                "After the turn, verify the mission remains open until the rehydrate check is done."
                    .to_string(),
            ],
        )
        .unwrap();

        let current = engine
            .stored_mission_state(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap()
            .expect("missing stored mission state");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE mission_states
             SET mission_status = ?1, continuation_mode = ?2, closure_confidence = ?3,
                 is_open = ?4, allow_idle = ?5, focus_head_commit_id = ?6
             WHERE conversation_id = ?7",
            rusqlite::params![
                "done",
                "continuous",
                "high",
                0,
                1,
                current.focus_head_commit_id,
                turn_loop::CHAT_CONVERSATION_ID,
            ],
        )
        .unwrap();
        drop(conn);

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired =
            run_turn_end_state_invariant_check(&root, &state, turn_loop::CHAT_CONVERSATION_ID)
                .expect("turn-end repair should return mission state");
        assert_eq!(repaired.mission_status, "active");
        assert!(repaired.is_open);
        assert!(!repaired.allow_idle);

        let report = state_invariants::evaluate_runtime_state_invariants(
            &root,
            turn_loop::CHAT_CONVERSATION_ID,
        )
        .expect("failed to evaluate invariants after turn-end repair");
        assert!(
            report.is_clean(),
            "unexpected violations: {:?}",
            report.violations
        );

        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events.iter().any(|event| {
            event.mechanism_id == "state_invariant_guard"
                && event.reason == "turn_state_invariants_repaired"
                && matches!(
                    event.action_taken.as_str(),
                    "reopened_mission_state_for_open_runtime_work"
                        | "canonicalized_focus_and_reopened_mission_state"
                )
        }));
    }

    #[test]
    fn turn_end_state_invariant_check_rebuilds_focus_after_refresh_skip() {
        let root = temp_root("turn-state-focus-refresh-skip");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let db_path = crate::paths::core_db(&root);
        let engine = LcmEngine::open(&db_path, LcmConfig::default()).unwrap();
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap();
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                ContinuityKind::Focus,
                "## Status\n+ Mission: Keep spilled work parked until restore.\n+ Mission state: done.\n+ Continuation mode: closed.\n+ Trigger intensity: warm.\n## Blocker\n+ Current blocker: none.\n## Next\n+ Next slice: none.\n## Done / Gate\n+ Done gate: work is already closed.\n+ Closure confidence: high.\n",
            )
            .unwrap();
        plan::handle_plan_command(
            &root,
            &[
                "ingest".to_string(),
                "--title".to_string(),
                "ticket spill restore: Deferred documentation review".to_string(),
                "--prompt".to_string(),
                "Restore the spilled queue task after queue pressure drops.".to_string(),
            ],
        )
        .unwrap();

        let before = state_invariants::evaluate_runtime_state_invariants(
            &root,
            turn_loop::CHAT_CONVERSATION_ID,
        )
        .expect("failed to evaluate initial invariants");
        assert!(before
            .violations
            .iter()
            .any(|issue| { issue.code == "closed_mission_with_open_runtime_work" }));

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired =
            run_turn_end_state_invariant_check(&root, &state, turn_loop::CHAT_CONVERSATION_ID)
                .expect("turn-end repair should return mission state");
        assert_eq!(repaired.mission_status, "active");
        assert!(repaired.is_open);
        assert_eq!(repaired.continuation_mode, "continuous");

        let report = state_invariants::evaluate_runtime_state_invariants(
            &root,
            turn_loop::CHAT_CONVERSATION_ID,
        )
        .expect("failed to evaluate invariants after focus rebuild");
        assert!(
            report.is_clean(),
            "unexpected violations: {:?}",
            report.violations
        );

        let continuity = engine
            .stored_continuity_show_all(turn_loop::CHAT_CONVERSATION_ID)
            .expect("failed to reload continuity");
        assert_ne!(continuity.focus.head_commit_id, "contbase_1_focus");
        assert!(continuity.focus.content.contains("- Mission state: active"));
        assert!(continuity
            .focus
            .content
            .contains("- Continuation mode: continuous"));

        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events.iter().any(|event| {
            event.mechanism_id == "state_invariant_guard"
                && event.reason == "turn_state_invariants_repaired"
                && event.action_taken == "canonicalized_focus_and_reopened_mission_state"
        }));
    }

    #[test]
    fn turn_end_state_invariant_check_hydrates_sparse_open_focus_from_runtime_title() {
        let root = temp_root("turn-state-sparse-open-focus");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let db_path = crate::paths::core_db(&root);
        let engine = LcmEngine::open(&db_path, LcmConfig::default()).unwrap();
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap();
        plan::handle_plan_command(
            &root,
            &[
                "ingest".to_string(),
                "--title".to_string(),
                "Spill restore: Deferred documentation review".to_string(),
                "--prompt".to_string(),
                "Restore the spilled queue task after pressure drops.".to_string(),
            ],
        )
        .unwrap();

        let current = engine
            .stored_mission_state(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap()
            .expect("missing stored mission state");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE mission_states
             SET mission = ?1, mission_status = ?2, continuation_mode = ?3, trigger_intensity = ?4,
                 blocker = ?5, next_slice = ?6, done_gate = ?7, closure_confidence = ?8,
                 is_open = ?9, allow_idle = ?10, focus_head_commit_id = ?11
             WHERE conversation_id = ?12",
            rusqlite::params![
                "",
                "active",
                "continuous",
                "hot",
                "",
                "",
                "",
                "low",
                1,
                0,
                current.focus_head_commit_id,
                turn_loop::CHAT_CONVERSATION_ID,
            ],
        )
        .unwrap();
        drop(conn);

        let before = state_invariants::evaluate_runtime_state_invariants(
            &root,
            turn_loop::CHAT_CONVERSATION_ID,
        )
        .expect("failed to evaluate sparse-open invariants");
        assert!(before
            .violations
            .iter()
            .any(|issue| { issue.code == "mission_state_requires_continuity_resync" }));

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired =
            run_turn_end_state_invariant_check(&root, &state, turn_loop::CHAT_CONVERSATION_ID)
                .expect("turn-end repair should return mission state");
        assert_eq!(
            repaired.mission,
            "Spill restore: Deferred documentation review"
        );
        assert_eq!(
            repaired.next_slice,
            "Spill restore: Deferred documentation review"
        );
        assert!(repaired.is_open);

        let report = state_invariants::evaluate_runtime_state_invariants(
            &root,
            turn_loop::CHAT_CONVERSATION_ID,
        )
        .expect("failed to evaluate invariants after sparse-open repair");
        assert!(
            report.is_clean(),
            "unexpected violations: {:?}",
            report.violations
        );

        let continuity = engine
            .stored_continuity_show_all(turn_loop::CHAT_CONVERSATION_ID)
            .expect("failed to reload continuity");
        assert!(continuity
            .focus
            .content
            .contains("- Mission: Spill restore: Deferred documentation review"));
        assert!(continuity
            .focus
            .content
            .contains("- Next slice: Spill restore: Deferred documentation review"));
    }

    #[test]
    fn turn_end_state_invariant_check_repairs_partial_commit_focus_conflict() {
        let root = temp_root("turn-state-partial-commit");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let db_path = crate::paths::core_db(&root);
        let engine = LcmEngine::open(&db_path, LcmConfig::default()).unwrap();
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap();
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                ContinuityKind::Focus,
                "## Status\n+ Mission: Old continuity head before turn-end repair.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: warm.\n## Blocker\n+ Current blocker: the runtime mission cache still points at the old focus head.\n## Next\n+ Next slice: resync the newest continuity head after this turn.\n## Done / Gate\n+ Done gate: the newest focus head must become authoritative without dropping open work.\n+ Closure confidence: low.\n",
            )
            .unwrap();
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                ContinuityKind::Focus,
                "## Status\n+ Mission: Keep the newest focus head authoritative after turn-end repair.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker: the mission cache may still be partially committed to the old head.\n## Next\n+ Next slice: verify the newest head is now the live truth.\n## Done / Gate\n+ Done gate: canonicalize the newest focus head and leave one bounded continuation open.\n",
            )
            .unwrap();

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired =
            run_turn_end_state_invariant_check(&root, &state, turn_loop::CHAT_CONVERSATION_ID)
                .expect("turn-end repair should return mission state");
        assert_eq!(
            repaired.mission,
            "Keep the newest focus head authoritative after turn-end repair."
        );

        let report = state_invariants::evaluate_runtime_state_invariants(
            &root,
            turn_loop::CHAT_CONVERSATION_ID,
        )
        .expect("failed to evaluate invariants after turn-end partial-commit repair");
        assert!(
            report.is_clean(),
            "unexpected violations: {:?}",
            report.violations
        );
        assert_eq!(
            report.mission_state.mission,
            "Keep the newest focus head authoritative after turn-end repair."
        );

        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events.iter().any(|event| {
            event.mechanism_id == "state_invariant_guard"
                && event.reason == "turn_state_invariants_repaired"
                && event.action_taken == "canonicalized_focus_and_resynced_mission_state"
        }));
    }

    #[test]
    fn queue_guard_not_inserted_below_threshold() {
        let root = temp_root("queue-guard-below");
        let mut shared = SharedState::default();
        shared.pending_prompts = VecDeque::from([
            QueuedPrompt {
                prompt: "a".to_string(),
                goal: "a".to_string(),
                preview: "a".to_string(),
                source_label: "cron".to_string(),
                suggested_skill: None,
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: Vec::new(),
                thread_key: None,
                workspace_root: None,
                ticket_self_work_id: None,
                outbound_email: None,
                outbound_anchor: None,
            },
            QueuedPrompt {
                prompt: "b".to_string(),
                goal: "b".to_string(),
                preview: "b".to_string(),
                source_label: "cron".to_string(),
                suggested_skill: None,
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: Vec::new(),
                thread_key: None,
                workspace_root: None,
                ticket_self_work_id: None,
                outbound_email: None,
                outbound_anchor: None,
            },
        ]);

        ensure_queue_guard_locked(&root, &mut shared);

        assert!(shared
            .pending_prompts
            .iter()
            .all(|item| item.source_label != QUEUE_GUARD_SOURCE_LABEL));
    }

    #[test]
    fn outcome_recovery_does_not_claim_next_queued_prompt_before_enqueue() {
        let root = temp_root("outcome-recovery-next-prompt");
        let mut shared = SharedState::default();
        shared.pending_prompts.push_back(QueuedPrompt {
            prompt: "next durable task".to_string(),
            goal: "next durable task".to_string(),
            preview: "next durable task".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::next".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("terminal-bench".to_string()),
            workspace_root: Some(root.to_string_lossy().to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        });

        let next = maybe_start_next_queued_prompt_after_recovery_locked(&root, &mut shared, true);

        assert!(next.is_none());
        assert!(!shared.busy);
        assert_eq!(shared.pending_prompts.len(), 1);
        assert_eq!(
            shared.pending_prompts.front().unwrap().leased_message_keys,
            vec!["queue:system::next".to_string()]
        );
        assert!(shared
            .recent_events
            .iter()
            .any(|event| event.contains("outcome-witness recovery")));
    }

    #[test]
    fn ticket_events_route_into_service_queue_with_dry_run_case() {
        let root = temp_root("ticket-route");
        let remote = crate::mission::ticket_local_native::create_local_ticket(
            &root,
            "VPN outage",
            "Users cannot reach the VPN gateway.",
            Some("open"),
            Some("high"),
        )
        .expect("failed to create local ticket");
        tickets::sync_ticket_system(&root, "local").expect("failed to sync local tickets");
        tickets::handle_ticket_command(
            &root,
            &[
                "bundle-put".to_string(),
                "--label".to_string(),
                "support/vpn".to_string(),
                "--runbook-id".to_string(),
                "rb-vpn".to_string(),
                "--policy-id".to_string(),
                "pol-vpn".to_string(),
            ],
        )
        .expect("failed to create control bundle");
        tickets::handle_ticket_command(
            &root,
            &[
                "label-set".to_string(),
                "--ticket-key".to_string(),
                format!("local:{}", remote.ticket_id),
                "--label".to_string(),
                "support/vpn".to_string(),
            ],
        )
        .expect("failed to label ticket");
        crate::mission::ticket_local_native::add_local_comment(
            &root,
            &remote.ticket_id,
            "Fresh operator-facing follow-up after CTOX attach",
        )
        .expect("failed to add follow-up comment");
        tickets::sync_ticket_system(&root, "local").expect("failed to resync local tickets");

        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
        }

        route_ticket_events(
            &root,
            &state,
            &std::collections::HashSet::from(["local".to_string()]),
        )
        .expect("ticket routing should succeed");

        let shared = state.lock().expect("state poisoned");
        assert_eq!(shared.pending_prompts.len(), 1);
        let prompt = shared
            .pending_prompts
            .front()
            .expect("ticket prompt missing");
        assert_eq!(prompt.source_label, "ticket:local");
        assert_eq!(prompt.suggested_skill, None);
        assert_eq!(prompt.leased_ticket_event_keys.len(), 1);
        assert!(prompt.prompt.contains("[Ticket-Ereignis]"));
        assert!(prompt.prompt.contains("Dry-Run-Artefakt"));

        let cases = tickets::list_cases(&root, Some(&format!("local:{}", remote.ticket_id)), 8)
            .expect("failed to list ticket cases");
        assert_eq!(cases.len(), 1);
        assert!(matches!(
            cases[0].state.as_str(),
            "approval_pending" | "executable"
        ));
    }

    #[test]
    fn unlabeled_ticket_events_are_blocked_by_ticket_control_gate() {
        let root = temp_root("ticket-gate");
        let _remote = crate::mission::ticket_local_native::create_local_ticket(
            &root,
            "Unknown support issue",
            "Ticket arrives without a label contract.",
            Some("open"),
            Some("low"),
        )
        .expect("failed to create local ticket");
        tickets::sync_ticket_system(&root, "local").expect("failed to sync local tickets");
        crate::mission::ticket_local_native::add_local_comment(
            &root,
            &_remote.ticket_id,
            "Fresh unlabeled update after CTOX attach",
        )
        .expect("failed to add follow-up comment");
        tickets::sync_ticket_system(&root, "local").expect("failed to resync local tickets");
        let state = Arc::new(Mutex::new(SharedState::default()));

        route_ticket_events(
            &root,
            &state,
            &std::collections::HashSet::from(["local".to_string()]),
        )
        .expect("ticket routing should succeed");

        let shared = state.lock().expect("state poisoned");
        assert!(shared.pending_prompts.is_empty());
        assert!(shared
            .recent_events
            .iter()
            .any(|item| item.contains("Blocked ticket event")));
        drop(shared);

        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events
            .iter()
            .any(|event| event.mechanism_id == "ticket_control_gate"));
    }

    #[test]
    fn queue_tasks_preserve_suggested_skill_into_service_queue() {
        let root = temp_root("queue-skill");
        channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Onboard ticket system".to_string(),
                prompt: "Inspect the attached ticket system and start the onboarding skill."
                    .to_string(),
                thread_key: "queue/onboarding".to_string(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: Some("system-onboarding".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
        }

        route_external_messages(&root, &state).expect("routing should succeed");

        let shared = state.lock().expect("state poisoned");
        assert_eq!(shared.pending_prompts.len(), 1);
        let prompt = shared
            .pending_prompts
            .front()
            .expect("queued prompt missing");
        assert_eq!(prompt.suggested_skill.as_deref(), Some("system-onboarding"));
        assert_eq!(prompt.source_label, "queue");
        assert!(shared
            .recent_events
            .iter()
            .any(|event| event.contains("skill system-onboarding")));
    }

    #[test]
    fn render_ticket_prompt_surfaces_source_skill_query_commands() {
        let root = temp_root("ticket-prompt-source-skill");
        std::fs::create_dir_all(&root).expect("temp root");
        let prompt = render_ticket_prompt(
            &root,
            &tickets::RoutedTicketEvent {
                event_key: "evt-1".to_string(),
                ticket_key: "zammad:123".to_string(),
                source_system: "zammad".to_string(),
                remote_event_id: "comment-1".to_string(),
                event_type: "comment".to_string(),
                summary: "Benutzer meldet Austritt und Zugriffsentzug".to_string(),
                body_text: "Mitarbeiteraustritt, bitte Konten ehemaliger Mitarbeiter deaktivieren."
                    .to_string(),
                title: "Deaktivierung für Konten ehemaliger Mitarbeiter".to_string(),
                remote_status: "open".to_string(),
                label: "support/access".to_string(),
                bundle_label: "support/access".to_string(),
                bundle_version: 1,
                case_id: "case-1".to_string(),
                dry_run_id: "dry-1".to_string(),
                dry_run_artifact: json!({"ok": true}),
                support_mode: "support_case".to_string(),
                approval_mode: "human_approval_required".to_string(),
                autonomy_level: "A0".to_string(),
                risk_level: "unknown".to_string(),
                thread_key: "ticket:zammad:123".to_string(),
            },
        );
        assert!(prompt.contains("Desk-Skill ansehen:"));
        assert!(prompt.contains("ticket source-skill-show --system zammad"));
        assert!(prompt.contains("ticket source-skill-query --system zammad"));
        assert!(prompt.contains("ticket source-skill-resolve --case-id case-1 --top-k 3"));
        assert!(prompt.contains(
            "ticket source-skill-compose-reply --case-id case-1 --send-policy suggestion"
        ));
        assert!(prompt.contains("ticket source-skill-review-note --case-id case-1"));
        assert!(prompt.contains("Oeffentliche Ticketantwort:"));
        assert!(prompt
            .contains("ticket writeback-comment --case-id case-1 --body \\\"<reply text>\\\""));
        assert!(prompt.contains("--internal"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn starting_queued_prompt_preserves_skill_in_recent_events() {
        let root = temp_root("ctox-starting-queued-prompt-skill");
        let mut shared = SharedState::default();
        shared.pending_prompts.push_back(QueuedPrompt {
            preview: "review onboarding".to_string(),
            source_label: "ticket:zammad".to_string(),
            goal: "continue onboarding".to_string(),
            prompt: "prompt".to_string(),
            suggested_skill: Some("system-onboarding".to_string()),
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        });

        let next = maybe_start_next_queued_prompt_locked(&root, &mut shared)
            .expect("queued prompt should be started");

        assert_eq!(next.suggested_skill.as_deref(), Some("system-onboarding"));
        assert!(shared.busy);
        assert_eq!(shared.active_source_label.as_deref(), Some("ticket:zammad"));
        assert!(shared.recent_events.iter().any(|event| {
            event.contains("Started queued ticket:zammad prompt [skill system-onboarding]")
        }));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn published_self_work_tickets_preserve_skill_hint_when_routed() {
        let root = temp_root("ticket-self-work-skill");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "system-onboarding".to_string(),
                title: "Review current helpdesk working model".to_string(),
                body_text: "Review the attached ticket desk and record onboarding gaps."
                    .to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "skill": "system-onboarding",
                    "phase": "observe",
                }),
            },
            true,
        )
        .expect("failed to create self-work item");
        let remote_ticket_id = item
            .remote_ticket_id
            .as_deref()
            .expect("remote ticket id missing")
            .to_string();

        tickets::sync_ticket_system(&root, "local").expect("failed to sync local tickets");
        tickets::handle_ticket_command(
            &root,
            &[
                "bundle-put".to_string(),
                "--label".to_string(),
                "support/onboarding".to_string(),
                "--runbook-id".to_string(),
                "rb-onboarding".to_string(),
                "--policy-id".to_string(),
                "pol-onboarding".to_string(),
            ],
        )
        .expect("failed to create bundle");
        tickets::handle_ticket_command(
            &root,
            &[
                "label-set".to_string(),
                "--ticket-key".to_string(),
                format!("local:{remote_ticket_id}"),
                "--label".to_string(),
                "support/onboarding".to_string(),
            ],
        )
        .expect("failed to assign label");
        crate::mission::ticket_local_native::add_local_comment(
            &root,
            &remote_ticket_id,
            "Please continue the onboarding review with the latest observations.",
        )
        .expect("failed to add follow-up comment");
        tickets::sync_ticket_system(&root, "local").expect("failed to resync local tickets");

        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
        }

        route_ticket_events(
            &root,
            &state,
            &std::collections::HashSet::from(["local".to_string()]),
        )
        .expect("ticket routing should succeed");

        let shared = state.lock().expect("state poisoned");
        assert_eq!(shared.pending_prompts.len(), 1);
        let prompt = shared
            .pending_prompts
            .front()
            .expect("queued prompt missing");
        assert_eq!(prompt.source_label, "ticket:local");
        assert_eq!(prompt.suggested_skill.as_deref(), Some("system-onboarding"));
        assert!(shared
            .recent_events
            .iter()
            .any(|event| event.contains("skill system-onboarding")));
    }

    #[test]
    fn published_self_work_tickets_route_without_manual_label_bundle() {
        let root = temp_root("ticket-self-work-synthetic-control");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "execution-enrichment-review".to_string(),
                title: "Build first execution source".to_string(),
                body_text:
                    "Continue the first execution-source review for the attached ticket system."
                        .to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "skill": "system-onboarding",
                    "phase": "desk-guided",
                }),
            },
            true,
        )
        .expect("failed to create self-work item");
        let remote_ticket_id = item
            .remote_ticket_id
            .as_deref()
            .expect("remote ticket id missing")
            .to_string();

        tickets::sync_ticket_system(&root, "local").expect("failed to sync local tickets");
        crate::mission::ticket_local_native::add_local_comment(
            &root,
            &remote_ticket_id,
            "Please continue this onboarding execution work.",
        )
        .expect("failed to add follow-up comment");
        tickets::sync_ticket_system(&root, "local").expect("failed to resync local tickets");

        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
        }

        route_ticket_events(
            &root,
            &state,
            &std::collections::HashSet::from(["local".to_string()]),
        )
        .expect("ticket routing should succeed");

        let shared = state.lock().expect("state poisoned");
        assert_eq!(shared.pending_prompts.len(), 1);
        let prompt = shared
            .pending_prompts
            .front()
            .expect("queued prompt missing");
        assert_eq!(prompt.source_label, "ticket:local");
        assert_eq!(prompt.suggested_skill.as_deref(), Some("system-onboarding"));
        assert!(shared
            .recent_events
            .iter()
            .any(|event| event.contains("skill system-onboarding")));
    }

    #[test]
    fn assigned_published_self_work_is_proactively_queued() {
        let root = temp_root("ticket-self-work-queue");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "execution-enrichment-review".to_string(),
                title: "Enrich first execution source".to_string(),
                body_text: "Build the first execution supplement for the attached source."
                    .to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "skill": "system-onboarding",
                }),
            },
            true,
        )
        .expect("failed to create self-work item");
        tickets::assign_ticket_self_work_item(&root, &item.work_id, "self", "ctox", None)
            .expect("failed to assign self-work");

        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
        }

        route_external_messages(&root, &state).expect("routing should succeed");

        let shared = state.lock().expect("state poisoned");
        assert_eq!(shared.pending_prompts.len(), 1);
        let prompt = shared
            .pending_prompts
            .front()
            .expect("queued prompt missing");
        assert_eq!(prompt.suggested_skill.as_deref(), Some("system-onboarding"));
        assert_eq!(
            prompt.ticket_self_work_id.as_deref(),
            Some(item.work_id.as_str())
        );
        let expected_thread_key = format!("ticket-self-work:{}", item.work_id);
        assert_eq!(
            prompt.thread_key.as_deref(),
            Some(expected_thread_key.as_str())
        );
        drop(shared);
        let routed = tickets::load_ticket_self_work_item(&root, &item.work_id)
            .expect("failed to reload self-work")
            .expect("self-work missing after routing");
        assert_eq!(routed.state, "queued");
        let shared = state.lock().expect("state poisoned");
        assert!(shared
            .recent_events
            .iter()
            .any(|event| event.contains("Queued self-work")));
    }

    #[test]
    fn source_skill_binding_guides_live_ticket_routing_when_no_self_work_skill() {
        let root = temp_root("ticket-source-skill");
        let remote = crate::mission::ticket_local_native::create_local_ticket(
            &root,
            "Sperrung MHS Benutzer GAJ",
            "Der Benutzer ist nach mehreren Fehlversuchen weiterhin gesperrt.",
            Some("open"),
            Some("high"),
        )
        .expect("failed to create local ticket");
        tickets::sync_ticket_system(&root, "local").expect("failed to sync local tickets");
        tickets::handle_ticket_command(
            &root,
            &[
                "bundle-put".to_string(),
                "--label".to_string(),
                "support/access".to_string(),
                "--runbook-id".to_string(),
                "rb-access".to_string(),
                "--policy-id".to_string(),
                "pol-access".to_string(),
            ],
        )
        .expect("failed to create bundle");
        tickets::handle_ticket_command(
            &root,
            &[
                "label-set".to_string(),
                "--ticket-key".to_string(),
                format!("local:{}", remote.ticket_id),
                "--label".to_string(),
                "support/access".to_string(),
            ],
        )
        .expect("failed to assign label");
        tickets::put_ticket_source_skill_binding(
            &root,
            "local",
            "roller-ticket-desk-operator-v4",
            "operating-model",
            "active",
            "ticket-onboarding",
            Some("runtime/generated-skills/roller-ticket-desk-operator-v4"),
            Some("Use desk-specific operating model for live ticket handling."),
        )
        .expect("failed to set source skill binding");
        crate::mission::ticket_local_native::add_local_comment(
            &root,
            &remote.ticket_id,
            "Bitte prüfen, ob das Kurzzeichen wieder entsperrt werden kann.",
        )
        .expect("failed to add local comment");
        tickets::sync_ticket_system(&root, "local").expect("failed to resync local tickets");

        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
        }

        route_ticket_events(
            &root,
            &state,
            &std::collections::HashSet::from(["local".to_string()]),
        )
        .expect("ticket routing should succeed");

        let shared = state.lock().expect("state poisoned");
        assert_eq!(shared.pending_prompts.len(), 1);
        let prompt = shared
            .pending_prompts
            .front()
            .expect("queued prompt missing");
        assert_eq!(
            prompt.suggested_skill.as_deref(),
            Some("roller-ticket-desk-operator-v4")
        );
        assert!(shared
            .recent_events
            .iter()
            .any(|event| event.contains("skill roller-ticket-desk-operator-v4")));
    }

    #[test]
    fn chat_submit_preserves_workspace_root_from_prompt_when_queued() {
        let root = temp_root("chat-submit-workspace");
        let mut shared = SharedState::default();
        shared.busy = true;
        let state = Arc::new(Mutex::new(shared));
        let prompt = "Work only inside this workspace:\n/tmp/ctox-cpp-smoke\n\nCreate main.cpp, build it, and verify the binary output.";

        let response = handle_service_ipc_request(
            ServiceIpcRequest::ChatSubmit {
                prompt: prompt.to_string(),
                thread_key: None,
                outbound_email: None,
                outbound_anchor: None,
            },
            &root,
            state.clone(),
        )
        .expect("chat submit should be accepted");

        match response {
            ServiceIpcResponse::Accepted(response) => {
                assert!(response.accepted);
                assert_eq!(response.status, "queued");
            }
            other => panic!("unexpected response: {other:?}"),
        }

        let shared = lock_shared_state(&state);
        assert_eq!(shared.pending_prompts.len(), 1);
        assert_eq!(
            shared.pending_prompts[0].workspace_root.as_deref(),
            Some("/tmp/ctox-cpp-smoke")
        );
    }

    #[test]
    fn chat_submit_preserves_explicit_thread_key_when_queued() {
        let root = temp_root("chat-submit-thread-key");
        let mut shared = SharedState::default();
        shared.busy = true;
        let state = Arc::new(Mutex::new(shared));

        let response = handle_service_ipc_request(
            ServiceIpcRequest::ChatSubmit {
                prompt: "Create src/main.cpp".to_string(),
                thread_key: Some("smoke/cpp-thread".to_string()),
                outbound_email: None,
                outbound_anchor: None,
            },
            &root,
            state.clone(),
        )
        .expect("chat submit should be accepted");

        match response {
            ServiceIpcResponse::Accepted(response) => {
                assert!(response.accepted);
                assert_eq!(response.status, "queued");
            }
            other => panic!("unexpected response: {other:?}"),
        }

        let shared = lock_shared_state(&state);
        assert_eq!(shared.pending_prompts.len(), 1);
        assert_eq!(
            shared.pending_prompts[0].thread_key.as_deref(),
            Some("smoke/cpp-thread")
        );
    }

    #[test]
    fn chat_submit_auto_ingests_prompt_secrets_before_queueing() {
        let root = temp_root("chat-submit-secrets");
        let mut shared = SharedState::default();
        shared.busy = true;
        let state = Arc::new(Mutex::new(shared));

        let response = handle_service_ipc_request(
            ServiceIpcRequest::ChatSubmit {
                prompt: "openAI API key:\nsk-proj-service-secret-1234567890".to_string(),
                thread_key: None,
                outbound_email: None,
                outbound_anchor: None,
            },
            &root,
            state.clone(),
        )
        .expect("chat submit should be accepted");

        match response {
            ServiceIpcResponse::Accepted(response) => {
                assert!(response.accepted);
                assert_eq!(response.status, "queued");
            }
            other => panic!("unexpected response: {other:?}"),
        }

        let shared = lock_shared_state(&state);
        assert_eq!(shared.pending_prompts.len(), 1);
        assert!(!shared.pending_prompts[0]
            .prompt
            .contains("sk-proj-service-secret-1234567890"));
        assert!(shared.pending_prompts[0]
            .prompt
            .contains("[secret-ref:credentials/OPENAI_API_KEY"));
        assert_eq!(
            shared.pending_prompts[0].suggested_skill.as_deref(),
            Some("secret-hygiene")
        );
        drop(shared);

        assert_eq!(
            secrets::read_secret_value(&root, "credentials", "OPENAI_API_KEY")
                .expect("secret should be readable"),
            "sk-proj-service-secret-1234567890"
        );
    }

    #[test]
    fn prepare_chat_prompt_suggests_secret_hygiene_when_auto_intake_runs() {
        let root = temp_root("prepare-chat-secret-skill");

        let prepared =
            prepare_chat_prompt(&root, "openAI API key:\nsk-proj-service-secret-1234567890")
                .expect("prompt preparation should succeed");

        assert_eq!(prepared.suggested_skill.as_deref(), Some("secret-hygiene"));
        assert!(prepared.auto_ingested_secrets >= 1);
        assert!(prepared
            .prompt
            .contains("[secret-ref:credentials/OPENAI_API_KEY"));
    }

    #[test]
    fn parse_service_status_accepts_missing_newer_fields() {
        let root = temp_root("status-compat");
        let body = r#"{
            "running": true,
            "busy": false,
            "pid": 1234,
            "listen_addr": "127.0.0.1:12435",
            "autostart_enabled": false,
            "manager": "process",
            "pending_count": 0,
            "active_source_label": null,
            "recent_events": ["ready"],
            "last_error": null,
            "last_completed_at": null,
            "last_reply_chars": null
        }"#;

        let status = parse_service_status(body, &root).unwrap();

        assert!(status.running);
        assert_eq!(status.listen_addr, "127.0.0.1:12435");
        assert!(status.pending_previews.is_empty());
        assert_eq!(status.current_goal_preview, None);
        assert_eq!(status.recent_events, vec!["ready".to_string()]);
    }

    #[test]
    fn service_status_surfaces_blocked_queue_tasks() {
        let root = temp_root("status-blocked-queue");
        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "HY3 smoke artifact missing".to_string(),
                prompt: "Create the missing smoke artifact.".to_string(),
                thread_key: "queue/status-blocked".to_string(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create queue task");
        channels::set_queue_task_route_status(&root, &task.message_key, "blocked")
            .expect("failed to block queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));

        let status = status_from_shared_state(&root, &state).expect("status should load");

        assert_eq!(status.pending_count, 1);
        assert!(status
            .pending_previews
            .iter()
            .any(|preview| preview.contains("queue blocked  HY3 smoke artifact missing")));
    }

    #[cfg(unix)]
    #[test]
    fn service_status_prefers_live_socket_even_when_systemd_marker_exists() {
        let root = std::path::PathBuf::from(format!(
            "/tmp/ctox-svc-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::fs::write(
            runtime_dir.join("ctox_systemd_user.installed"),
            "installed\n",
        )
        .unwrap();
        let socket_path = service_socket_path(&root);
        let listener = UnixListener::bind(&socket_path).unwrap();
        let response = serde_json::to_string(&ServiceIpcResponse::Status(ServiceStatus {
            running: true,
            busy: false,
            pid: Some(4242),
            listen_addr: service_listen_addr(&root),
            autostart_enabled: false,
            manager: "process".to_string(),
            pending_count: 1,
            pending_previews: vec!["ticket  support/onboarding zammad:42".to_string()],
            current_goal_preview: Some("Inspect onboarding ticket".to_string()),
            active_source_label: Some("ticket:zammad".to_string()),
            recent_events: vec!["Started prompt [skill system-onboarding]".to_string()],
            last_error: None,
            last_completed_at: None,
            last_reply_chars: None,
            monitor_last_check_at: None,
            monitor_alerts: Vec::new(),
            monitor_last_error: None,
            last_agent_outcome: None,
            work_hours: crate::service::working_hours::snapshot(&root),
        }))
        .unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            reader.read_line(&mut request).unwrap();
            assert!(request.contains("\"status\""));
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(b"\n").unwrap();
            stream.flush().unwrap();
        });

        let status = service_status_snapshot(&root).unwrap();
        handle.join().unwrap();

        assert!(status.running);
        assert_eq!(status.manager, "process");
        assert_eq!(status.pid, Some(4242));
        assert_eq!(status.pending_count, 1);
        assert!(status
            .recent_events
            .iter()
            .any(|event| event.contains("system-onboarding")));
    }

    #[test]
    fn systemd_user_unit_installed_requires_matching_root_when_only_global_unit_exists() {
        let temp_home = std::env::temp_dir().join(format!(
            "ctox-systemd-root-match-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let config_dir = temp_home.join(".config/systemd/user");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(
            config_dir.join(SYSTEMD_USER_UNIT_NAME),
            "[Service]\nWorkingDirectory=/srv/ctox-installed\nEnvironment=CTOX_ROOT=/srv/ctox-installed\n",
        )
        .unwrap();
        let mismatched_root = temp_home.join("isolated-root");
        std::fs::create_dir_all(mismatched_root.join("runtime")).unwrap();

        let original_home = std::env::var_os("HOME");
        let original_xdg = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("HOME", &temp_home);
        std::env::remove_var("XDG_CONFIG_HOME");
        let installed = systemd_user_unit_installed(&mismatched_root);
        match original_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match original_xdg {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }

        assert!(!installed);
    }

    #[cfg(unix)]
    #[test]
    fn service_status_socket_tolerates_slow_status_response() {
        let root = std::path::PathBuf::from(format!(
            "/tmp/ctox-svc-slow-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let socket_path = service_socket_path(&root);
        let listener = UnixListener::bind(&socket_path).unwrap();
        let response = serde_json::to_string(&ServiceIpcResponse::Status(ServiceStatus {
            running: true,
            busy: false,
            pid: Some(5151),
            listen_addr: service_listen_addr(&root),
            autostart_enabled: false,
            manager: "process".to_string(),
            pending_count: 0,
            pending_previews: Vec::new(),
            current_goal_preview: None,
            active_source_label: None,
            recent_events: vec!["slow status ok".to_string()],
            last_error: None,
            last_completed_at: None,
            last_reply_chars: None,
            monitor_last_check_at: None,
            monitor_alerts: Vec::new(),
            monitor_last_error: None,
            last_agent_outcome: None,
            work_hours: crate::service::working_hours::snapshot(&root),
        }))
        .unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            reader.read_line(&mut request).unwrap();
            std::thread::sleep(Duration::from_secs(2));
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(b"\n").unwrap();
            stream.flush().unwrap();
        });

        let status = service_status_snapshot(&root).unwrap();
        handle.join().unwrap();

        assert!(status.running);
        assert_eq!(status.pid, Some(5151));
        assert_eq!(status.recent_events, vec!["slow status ok".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn service_chat_submit_socket_tolerates_slow_accept_response() {
        let root = std::path::PathBuf::from(format!(
            "/tmp/ctox-chat-slow-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let socket_path = service_socket_path(&root);
        let listener = UnixListener::bind(&socket_path).unwrap();
        let response = serde_json::to_string(&ServiceIpcResponse::Accepted(AcceptedResponse {
            accepted: true,
            status: "started".to_string(),
        }))
        .unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            reader.read_line(&mut request).unwrap();
            assert!(request.contains("\"chat_submit\""));
            std::thread::sleep(Duration::from_secs(1));
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(b"\n").unwrap();
            stream.flush().unwrap();
        });

        let accepted = send_service_ipc_request(
            &root,
            ServiceIpcRequest::ChatSubmit {
                prompt: "Run an internal harness self-check.".to_string(),
                thread_key: None,
                outbound_email: None,
                outbound_anchor: None,
            },
        )
        .unwrap();
        handle.join().unwrap();

        match accepted {
            ServiceIpcResponse::Accepted(response) => {
                assert!(response.accepted);
                assert_eq!(response.status, "started");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn timeout_continuation_prompt_summarizes_nested_goal() {
        let prompt = render_timeout_continue_prompt(
            "Continue the interrupted task from the latest durable state instead of treating it as externally blocked.\n\nGoal:\nMission continuity watchdog detected an open mission that went idle for 45 seconds.\n\nThe previous slice stopped because it hit the turn time budget:\nexecution timed out after 900s",
            "execution timed out after 900s",
            None,
        );
        assert!(
            prompt.contains("CURRENT TASK\nMission continuity watchdog detected an open mission")
        );
        assert!(prompt.contains("STOP REASON\nexecution timed out after 900s"));
        assert!(prompt.contains("Preserve work that already exists"));
        assert!(!prompt.contains("The previous slice stopped because it hit the turn time budget:\nexecution timed out after 900s\n\nThe previous slice stopped"));
    }

    #[test]
    fn workspace_contract_is_prepended_for_workspace_scoped_prompts() {
        let prompt = prepend_workspace_contract(
            "Implement the requested slice.",
            Some("/tmp/ctox-workspace-contract"),
        );
        assert!(prompt
            .starts_with("Work only inside this workspace:\n/tmp/ctox-workspace-contract\n\n"));
        assert!(prompt.contains("Implement the requested slice."));

        let timeout_prompt = render_timeout_continue_prompt(
            "Ship the next implementation slice.",
            "execution timed out after 180s",
            Some("/tmp/ctox-workspace-contract"),
        );
        assert!(timeout_prompt
            .contains("Work only inside this workspace:\n/tmp/ctox-workspace-contract"));
        assert!(timeout_prompt.contains("Slice goal:\nShip the next implementation slice."));
    }

    #[test]
    fn blocks_non_owner_email_instructions() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@example.com".to_string(),
        );
        settings.insert(
            "CTOX_ALLOWED_EMAIL_DOMAIN".to_string(),
            "example.com".to_string(),
        );
        let message = channels::RoutedInboundMessage {
            message_key: "m1".to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "t1".to_string(),
            sender_display: "Mallory".to_string(),
            sender_address: "mallory@example.com".to_string(),
            subject: "test".to_string(),
            preview: "test".to_string(),
            body_text: "test".to_string(),
            external_created_at: "2026-03-26T00:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };

        assert_eq!(
            blocked_inbound_reason(&message, &settings),
            Some(
                "sender is outside the configured founder/owner/admin list and allowed employee email domain"
                    .to_string()
            )
        );
    }

    #[test]
    fn allows_domain_user_email_instructions() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@example.com".to_string(),
        );
        settings.insert(
            "CTOX_ALLOWED_EMAIL_DOMAIN".to_string(),
            "example.com".to_string(),
        );
        let message = channels::RoutedInboundMessage {
            message_key: "m1".to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "t1".to_string(),
            sender_display: "Alice".to_string(),
            sender_address: "alice@example.com".to_string(),
            subject: "test".to_string(),
            preview: "test".to_string(),
            body_text: "test".to_string(),
            external_created_at: "2026-03-26T00:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };

        assert_eq!(blocked_inbound_reason(&message, &settings), None);
    }

    #[test]
    fn founder_dashboard_url_token_does_not_block_inbound_reply() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "mp@iip-gmbh.de".to_string(),
        );
        let mut message = routed_email_message("mp@iip-gmbh.de");
        message.subject = "AW: Kunstmen Wettbewerbsdashboard".to_string();
        message.preview = "Danke, hier ist der zitierte Dashboard-Link".to_string();
        message.body_text =
            "Danke.\n\n> https://www.kunstmen.com/internal/competitors?token=abc123".to_string();

        assert_eq!(blocked_inbound_reason(&message, &settings), None);
    }

    #[test]
    fn founder_api_token_text_still_blocks_inbound_reply() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "mp@iip-gmbh.de".to_string(),
        );
        let mut message = routed_email_message("mp@iip-gmbh.de");
        message.body_text = "api_token=abcd1234".to_string();

        assert_eq!(
            blocked_inbound_reason(&message, &settings),
            Some("secret-bearing input must move to TUI".to_string())
        );
    }

    #[test]
    fn meeting_auto_join_policy_blocks_disabled_and_unlisted_senders() {
        let message = routed_email_message("alice@example.com");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTO_MEETING_AUTO_JOIN_ENABLED".to_string(),
            "false".to_string(),
        );
        assert!(meeting_auto_join_policy_block(&settings, &message)
            .expect("disabled block")
            .contains("auto-join disabled"));

        settings.insert(
            "CTO_MEETING_AUTO_JOIN_ENABLED".to_string(),
            "true".to_string(),
        );
        settings.insert(
            "CTO_MEETING_ALLOWED_INVITE_SENDERS".to_string(),
            "scheduler@example.com,@trusted.example".to_string(),
        );
        assert!(meeting_auto_join_policy_block(&settings, &message)
            .expect("sender block")
            .contains("not in"));

        let allowed_exact = routed_email_message("scheduler@example.com");
        assert_eq!(
            meeting_auto_join_policy_block(&settings, &allowed_exact),
            None
        );
        let allowed_domain = routed_email_message("ops@trusted.example");
        assert_eq!(
            meeting_auto_join_policy_block(&settings, &allowed_domain),
            None
        );
    }

    #[test]
    fn meeting_passive_chat_is_acknowledged_without_agent_queue() {
        let root = temp_root("meeting-passive");
        upsert_test_inbound_message(
            &root,
            "meeting-passive-1",
            "meeting",
            "meeting-session-passive",
            "alice",
            "google meeting chat",
            "Nur ein normaler Chat.",
            json!({
                "source": "meeting_chat",
                "session_id": "meeting-session-passive",
                "is_mention": false,
                "priority": "normal"
            }),
        );
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
        }

        route_external_messages(&root, &state).expect("route meeting chat");

        let shared = state.lock().expect("state poisoned");
        assert!(shared.pending_prompts.is_empty());
        drop(shared);
        assert_eq!(route_status_for(&root, "meeting-passive-1"), "handled");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn non_work_tui_route_is_acknowledged_without_agent_queue() {
        let root = temp_root("tui-non-work-route");
        upsert_test_inbound_message(
            &root,
            "tui-probe-1",
            "tui",
            "tui/main",
            "owner",
            "Demo",
            "hello queue",
            json!({ "priority": "normal" }),
        );
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
        }

        route_external_messages(&root, &state).expect("route tui probe");

        let shared = state.lock().expect("state poisoned");
        assert!(shared.pending_prompts.is_empty());
        assert!(shared
            .recent_events
            .iter()
            .any(|event| event.contains("Ignored non-work TUI route")));
        drop(shared);
        assert_eq!(route_status_for(&root, "tui-probe-1"), "handled");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn meeting_mentions_are_prioritized_into_agent_queue() {
        let root = temp_root("meeting-mention");
        upsert_test_inbound_message(
            &root,
            "meeting-mention-1",
            "meeting",
            "meeting-session-mention",
            "alice",
            "google meeting chat",
            "@CTOX bitte pruefen.",
            json!({
                "source": "meeting_chat",
                "session_id": "meeting-session-mention",
                "is_mention": true,
                "skill": "meeting-participant",
                "priority": "urgent"
            }),
        );
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
        }

        route_external_messages(&root, &state).expect("route meeting mention");

        let shared = state.lock().expect("state poisoned");
        assert_eq!(shared.pending_prompts.len(), 1);
        let prompt = shared.pending_prompts.front().expect("queued mention");
        assert_eq!(prompt.source_label, "meeting:mention");
        assert_eq!(
            prompt.suggested_skill.as_deref(),
            Some("meeting-participant")
        );
        assert_eq!(prompt.leased_message_keys, vec!["meeting-mention-1"]);
        drop(shared);
        assert_eq!(route_status_for(&root, "meeting-mention-1"), "leased");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn allows_founder_email_outside_employee_domain() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@example.com".to_string(),
        );
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "founder@external.net,cofounder@startup.test".to_string(),
        );
        settings.insert(
            "CTOX_ALLOWED_EMAIL_DOMAIN".to_string(),
            "example.com".to_string(),
        );
        let message = channels::RoutedInboundMessage {
            message_key: "m-founder".to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "t-founder".to_string(),
            sender_display: "Founder".to_string(),
            sender_address: "cofounder@startup.test".to_string(),
            subject: "founder input".to_string(),
            preview: "founder input".to_string(),
            body_text: "founder input".to_string(),
            external_created_at: "2026-03-26T00:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };

        assert_eq!(blocked_inbound_reason(&message, &settings), None);
    }

    #[test]
    fn owner_email_inbound_gets_owner_source_label() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@example.com".to_string(),
        );
        let message = channels::RoutedInboundMessage {
            message_key: "m-owner".to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "t-owner".to_string(),
            sender_display: "Michael".to_string(),
            sender_address: "michael.welsch@example.com".to_string(),
            subject: "prio".to_string(),
            preview: "prio".to_string(),
            body_text: "prio".to_string(),
            external_created_at: "2026-03-26T00:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };

        assert_eq!(inbound_source_label(&settings, &message), "email:owner");
    }

    #[test]
    fn teams_inbound_gets_full_channel_prompt() {
        let root = temp_root("teams-inbound-prompt");
        let settings = BTreeMap::new();
        let message = routed_teams_message();

        let prompt = enrich_inbound_prompt(&root, &settings, &message, &message.body_text);

        assert!(prompt.contains("[Teams-Nachricht eingegangen]"));
        assert!(prompt.contains("ctox channel send --channel teams"));
        assert!(prompt.contains("--account-key teams:bot"));
        assert!(prompt.contains("--thread-key 'teams:bot::chat::chat-123'"));
        assert!(prompt.contains("Microsoft Graph"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn teams_inbound_participates_in_communication_priority() {
        assert_eq!(source_label_dispatch_rank("teams"), 3);
        assert_eq!(
            inbound_source_label(&BTreeMap::new(), &routed_teams_message()),
            "teams"
        );
    }

    #[test]
    fn founder_email_inbound_gets_founder_source_label() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@example.com".to_string(),
        );
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "founder@other.example, cofounder@startup.test".to_string(),
        );
        let message = channels::RoutedInboundMessage {
            message_key: "m-founder".to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "t-founder".to_string(),
            sender_display: "Founder".to_string(),
            sender_address: "cofounder@startup.test".to_string(),
            subject: "prio".to_string(),
            preview: "prio".to_string(),
            body_text: "prio".to_string(),
            external_created_at: "2026-03-26T00:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };

        assert_eq!(inbound_source_label(&settings, &message), "email:founder");
    }

    #[test]
    fn founder_email_inbound_uses_isolated_execution_thread_key() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "cofounder@startup.test".to_string(),
        );
        let message = channels::RoutedInboundMessage {
            message_key: "m-founder".to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "<founder-thread@example.com>".to_string(),
            sender_display: "Founder".to_string(),
            sender_address: "cofounder@startup.test".to_string(),
            subject: "prio".to_string(),
            preview: "prio".to_string(),
            body_text: "prio".to_string(),
            external_created_at: "2026-03-26T00:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };

        let derived = execution_thread_key_for_inbound_message(&settings, &message);
        assert!(derived.starts_with("email-review:founder:"));
        assert_ne!(derived, message.thread_key);
    }

    #[test]
    fn ordinary_email_inbound_keeps_original_execution_thread_key() {
        let settings = BTreeMap::new();
        let message = channels::RoutedInboundMessage {
            message_key: "m-email".to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "<plain-thread@example.com>".to_string(),
            sender_display: "External".to_string(),
            sender_address: "person@example.com".to_string(),
            subject: "hello".to_string(),
            preview: "hello".to_string(),
            body_text: "hello".to_string(),
            external_created_at: "2026-03-26T00:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };

        assert_eq!(
            execution_thread_key_for_inbound_message(&settings, &message),
            message.thread_key
        );
    }

    #[test]
    fn live_service_settings_include_process_env_founder_policy() {
        let root = temp_root("live-service-settings");
        let owner_key = "CTOX_OWNER_EMAIL_ADDRESS";
        let founder_key = "CTOX_FOUNDER_EMAIL_ADDRESSES";
        let roles_key = "CTOX_FOUNDER_EMAIL_ROLES";
        let allowed_key = "CTOX_ALLOWED_EMAIL_DOMAIN";
        let previous_owner = std::env::var_os(owner_key);
        let previous_founders = std::env::var_os(founder_key);
        let previous_roles = std::env::var_os(roles_key);
        let previous_allowed = std::env::var_os(allowed_key);

        std::env::set_var(owner_key, "michael.welsch@metric-space.ai");
        std::env::set_var(
            founder_key,
            "michael.welsch@metric-space.ai,o.schaefers@gmx.net",
        );
        std::env::set_var(
            roles_key,
            "michael.welsch@metric-space.ai=CEO / Founder,o.schaefers@gmx.net=Sales Officer",
        );
        std::env::set_var(allowed_key, "metric-space.ai");

        let settings = live_service_settings(&root);
        let founder_policy =
            channels::classify_email_sender(&settings, "michael.welsch@metric-space.ai");

        match previous_owner {
            Some(value) => std::env::set_var(owner_key, value),
            None => std::env::remove_var(owner_key),
        }
        match previous_founders {
            Some(value) => std::env::set_var(founder_key, value),
            None => std::env::remove_var(founder_key),
        }
        match previous_roles {
            Some(value) => std::env::set_var(roles_key, value),
            None => std::env::remove_var(roles_key),
        }
        match previous_allowed {
            Some(value) => std::env::set_var(allowed_key, value),
            None => std::env::remove_var(allowed_key),
        }

        assert_eq!(
            settings.get(owner_key).map(String::as_str),
            Some("michael.welsch@metric-space.ai")
        );
        assert_eq!(
            settings.get(founder_key).map(String::as_str),
            Some("michael.welsch@metric-space.ai,o.schaefers@gmx.net")
        );
        assert_eq!(founder_policy.role, "owner");
        assert!(founder_policy.allowed);
        assert!(founder_policy.allow_admin_actions);
    }

    #[test]
    fn ordered_pending_prompts_put_owner_email_ahead_of_queue_work() {
        let mut pending = VecDeque::new();
        insert_pending_prompt_ordered(
            &mut pending,
            QueuedPrompt {
                prompt: "legacy".to_string(),
                goal: "legacy".to_string(),
                preview: "legacy".to_string(),
                source_label: "queue".to_string(),
                suggested_skill: None,
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: Vec::new(),
                thread_key: None,
                workspace_root: None,
                ticket_self_work_id: None,
                outbound_email: None,
                outbound_anchor: None,
            },
        );
        insert_pending_prompt_ordered(
            &mut pending,
            QueuedPrompt {
                prompt: "owner".to_string(),
                goal: "owner".to_string(),
                preview: "owner".to_string(),
                source_label: "email:owner".to_string(),
                suggested_skill: None,
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: Vec::new(),
                thread_key: None,
                workspace_root: None,
                ticket_self_work_id: None,
                outbound_email: None,
                outbound_anchor: None,
            },
        );

        assert_eq!(
            pending.front().map(|item| item.source_label.as_str()),
            Some("email:owner")
        );
        assert_eq!(
            pending.back().map(|item| item.source_label.as_str()),
            Some("queue")
        );
    }

    #[test]
    fn ordered_pending_prompts_put_founder_email_ahead_of_queue_work() {
        let mut pending = VecDeque::new();
        insert_pending_prompt_ordered(
            &mut pending,
            QueuedPrompt {
                prompt: "legacy".to_string(),
                goal: "legacy".to_string(),
                preview: "legacy".to_string(),
                source_label: "queue".to_string(),
                suggested_skill: None,
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: Vec::new(),
                thread_key: None,
                workspace_root: None,
                ticket_self_work_id: None,
                outbound_email: None,
                outbound_anchor: None,
            },
        );
        insert_pending_prompt_ordered(
            &mut pending,
            QueuedPrompt {
                prompt: "founder".to_string(),
                goal: "founder".to_string(),
                preview: "founder".to_string(),
                source_label: "email:founder".to_string(),
                suggested_skill: None,
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: Vec::new(),
                thread_key: None,
                workspace_root: None,
                ticket_self_work_id: None,
                outbound_email: None,
                outbound_anchor: None,
            },
        );

        assert_eq!(
            pending.front().map(|item| item.source_label.as_str()),
            Some("email:founder")
        );
        assert_eq!(
            pending.back().map(|item| item.source_label.as_str()),
            Some("queue")
        );
    }

    #[test]
    fn blocks_secret_bearing_email_even_from_allowed_domain() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@example.com".to_string(),
        );
        settings.insert(
            "CTOX_ALLOWED_EMAIL_DOMAIN".to_string(),
            "example.com".to_string(),
        );
        let message = channels::RoutedInboundMessage {
            message_key: "m2".to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "t2".to_string(),
            sender_display: "Alice".to_string(),
            sender_address: "alice@example.com".to_string(),
            subject: "Nextcloud secret".to_string(),
            preview: "NEXTCLOUD_PASSWORD=supersecret".to_string(),
            body_text: "NEXTCLOUD_PASSWORD=supersecret".to_string(),
            external_created_at: "2026-03-26T00:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };

        assert_eq!(
            blocked_inbound_reason(&message, &settings),
            Some("secret-bearing input must move to TUI".to_string())
        );
    }

    #[test]
    fn admin_policy_distinguishes_sudo_rights() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@example.com".to_string(),
        );
        settings.insert(
            "CTOX_ALLOWED_EMAIL_DOMAIN".to_string(),
            "example.com".to_string(),
        );
        settings.insert(
            "CTOX_EMAIL_ADMIN_POLICIES".to_string(),
            "opsadmin@example.com:sudo,helpdesk@example.com:nosudo".to_string(),
        );
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "founder@external.net".to_string(),
        );

        let sudo_admin = channels::classify_email_sender(&settings, "opsadmin@example.com");
        assert_eq!(sudo_admin.role, "admin");
        assert!(sudo_admin.allow_admin_actions);
        assert!(sudo_admin.allow_sudo_actions);

        let plain_admin = channels::classify_email_sender(&settings, "helpdesk@example.com");
        assert_eq!(plain_admin.role, "admin");
        assert!(plain_admin.allow_admin_actions);
        assert!(!plain_admin.allow_sudo_actions);

        let founder = channels::classify_email_sender(&settings, "founder@external.net");
        assert_eq!(founder.role, "founder");
        assert!(founder.allow_admin_actions);
        assert!(!founder.allow_sudo_actions);

        let domain_user = channels::classify_email_sender(&settings, "user@example.com");
        assert_eq!(domain_user.role, "domain_user");
        assert!(domain_user.allowed);
        assert!(!domain_user.allow_admin_actions);
    }

    #[test]
    fn timeout_blocker_reuses_existing_same_thread_follow_up() {
        let root = std::env::temp_dir().join(format!(
            "ctox-timeout-followup-reuse-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("failed to create temp root");
        let existing = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "spill restore: Deferred documentation review".to_string(),
                prompt: "Keep exactly one open follow-up after the timeout.".to_string(),
                thread_key: "tui/main".to_string(),
                workspace_root: Some("/tmp/ctox-timeout-followup-test".to_string()),
                priority: "high".to_string(),
                suggested_skill: Some("queue-orchestrator".to_string()),
                parent_message_key: Some("queue-key-1".to_string()),
                extra_metadata: None,
            },
        )
        .expect("failed to seed follow-up");
        let job = QueuedPrompt {
            prompt: "Add mobile-first search".to_string(),
            goal:
                "Add mobile-first search expectations, map-based discovery, and a saved-search path"
                    .to_string(),
            preview: "Add mobile-first search".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: Some("change-lifecycle".to_string()),
            leased_message_keys: vec!["queue-key-1".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tui/main".to_string()),
            workspace_root: Some("/tmp/ctox-timeout-followup-test".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "execution timed out after 180s")
                .expect("timeout continuation should succeed");

        assert_eq!(created, None);
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].message_key, existing.message_key);
    }

    #[test]
    fn timeout_blocker_reuses_existing_workspace_follow_up_when_thread_differs() {
        let root = std::env::temp_dir().join(format!(
            "ctox-timeout-followup-workspace-reuse-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("failed to create temp root");
        let existing = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "spill restore: Restore monitoring follow-up".to_string(),
                prompt: "Reuse the restored follow-up instead of adding a timeout duplicate."
                    .to_string(),
                thread_key: "queue/rehydrate-existing".to_string(),
                workspace_root: Some("/tmp/ctox-timeout-followup-test".to_string()),
                priority: "high".to_string(),
                suggested_skill: Some("queue-orchestrator".to_string()),
                parent_message_key: Some("queue-key-1".to_string()),
                extra_metadata: None,
            },
        )
        .expect("failed to seed workspace follow-up");
        let job = QueuedPrompt {
            prompt: "Restore monitoring follow-up".to_string(),
            goal: "Restore monitoring follow-up from the latest durable spill state".to_string(),
            preview: "Restore monitoring follow-up".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: Some("queue-orchestrator".to_string()),
            leased_message_keys: vec!["queue-key-1".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tui/main".to_string()),
            workspace_root: Some("/tmp/ctox-timeout-followup-test".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "execution timed out after 180s")
                .expect("timeout continuation should succeed");

        assert_eq!(created, None);
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].message_key, existing.message_key);
    }

    #[test]
    fn timeout_blocker_suppresses_continuation_and_records_governance_event() {
        let root = std::env::temp_dir().join(format!(
            "ctox-timeout-followup-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("failed to create temp root");
        let job = QueuedPrompt {
            prompt: "Add mobile-first search".to_string(),
            goal:
                "Add mobile-first search expectations, map-based discovery, and a saved-search path"
                    .to_string(),
            preview: "Add mobile-first search".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: Some("change-lifecycle".to_string()),
            leased_message_keys: vec!["queue-key-1".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tui/main".to_string()),
            workspace_root: Some("/tmp/ctox-timeout-followup-test".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "execution timed out after 180s")
                .expect("timeout continuation should succeed");

        assert_eq!(created, None);
        let tasks = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list queue tasks");
        assert!(tasks.is_empty());
        let self_work = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work items");
        assert!(self_work.is_empty());
        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events
            .iter()
            .any(|event| event.mechanism_id == "turn_timeout_continuation"));
    }

    #[test]
    fn timeout_blocker_queues_durable_artifact_controller_recovery() {
        let root = temp_root("ctox-timeout-durable-artifact-controller");
        let workspace = root.join("terminal-bench-run");
        std::fs::create_dir_all(&workspace).expect("failed to create workspace");
        let controller = workspace.join("controller.json");
        let logbook = workspace.join("run-log.md");
        let job = QueuedPrompt {
            prompt: format!(
                "Terminal-Bench 2 controller via Harbor.\n\nOnly required durable files for this controller turn:\n- {}\n- {}\n\nKeep these artifacts updated while running benchmark tickets.",
                controller.display(),
                logbook.display()
            ),
            goal: "Run Terminal-Bench 2 controller and write durable results".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::current".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tb2-controller".to_string()),
            workspace_root: Some(workspace.to_string_lossy().into_owned()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "direct session timeout after 900s")
                .expect("timeout recovery should succeed");

        assert!(created
            .as_deref()
            .unwrap_or_default()
            .starts_with("Recover interrupted Run Terminal-Bench 2"));
        let pending = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list pending queue tasks");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].thread_key, "tb2-controller");
        assert_eq!(
            pending[0].workspace_root.as_deref(),
            job.workspace_root.as_deref()
        );
        assert_eq!(pending[0].parent_message_key, None);
        assert!(pending[0]
            .prompt
            .contains("DURABLE FILES THAT MUST STAY UPDATED"));
        assert!(pending[0].prompt.contains(controller.to_str().unwrap()));
        assert!(pending[0].prompt.contains(logbook.to_str().unwrap()));
        let self_work = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work items");
        assert!(self_work.is_empty());
    }

    #[test]
    fn timeout_blocker_queues_review_feedback_controller_recovery_without_lease() {
        let root = temp_root("ctox-timeout-review-feedback-controller");
        let workspace = root.join("terminal-bench-run");
        std::fs::create_dir_all(&workspace).expect("failed to create workspace");
        let controller = workspace.join("controller.json");
        let logbook = workspace.join("logbook.md");
        let knowledge = workspace.join("knowledge.md");
        let results = workspace.join("results.jsonl");
        let parent = QueuedPrompt {
            prompt: format!(
                "Run Terminal-Bench 2 controller.\n\nOnly required durable files for this controller turn:\n- {}\n- {}\n- {}\n- {}\n",
                controller.display(),
                logbook.display(),
                knowledge.display(),
                results.display()
            ),
            goal: "Terminal-Bench 2 Qwen3.6 128k clean".to_string(),
            preview: "Terminal-Bench controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::parent".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tb2-controller".to_string()),
            workspace_root: Some(workspace.to_string_lossy().into_owned()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };
        let feedback_prompt = terminal_bench_controller_hold_feedback_prompt(
            &parent,
            "review timed out before accepting initialized artifacts",
        );
        let job = QueuedPrompt {
            prompt: feedback_prompt,
            goal: "Continue Terminal-Bench controller for queue".to_string(),
            preview: "HARNESS FEEDBACK Terminal-Bench controller continuation".to_string(),
            source_label: "review-feedback".to_string(),
            suggested_skill: parent.suggested_skill.clone(),
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: parent.thread_key.clone(),
            workspace_root: parent.workspace_root.clone(),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "direct session timeout after 900s")
                .expect("review-feedback timeout recovery should succeed");

        assert!(created
            .as_deref()
            .unwrap_or_default()
            .starts_with("Recover interrupted Continue Terminal-Bench controller"));
        let pending = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list pending queue tasks");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].thread_key, "tb2-controller");
        assert!(pending[0]
            .prompt
            .contains("TERMINAL-BENCH TIMEOUT RECOVERY ORDER"));
        assert!(pending[0]
            .prompt
            .contains("Immediately write a checkpoint into controller.json"));
        assert!(pending[0].prompt.contains(controller.to_str().unwrap()));
        assert!(pending[0].prompt.contains(results.to_str().unwrap()));
    }

    #[test]
    fn timeout_continuation_does_not_spawn_owner_visible_outbound_retry() {
        let root = temp_root("ctox-timeout-outbound-intent");
        let outbound = channels::FounderOutboundAction {
            account_key: "email:cto1@metric-space.ai".to_string(),
            thread_key: "founder-outbound:julia-tag-proposal".to_string(),
            subject: "Vorschlag Tag-System fuer Lead-Funnel in Salesforce".to_string(),
            to: vec!["j.kienzler@remcapital.de".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
        };
        let job = QueuedPrompt {
            prompt: "Schreibe und sende per reviewed-founder-send eine Mail an Julia.".to_string(),
            goal: "Tag-Proposal-Mail an Julia final senden".to_string(),
            preview: "reviewed-founder-send Julia".to_string(),
            source_label: "tui-outbound".to_string(),
            suggested_skill: Some("owner-communication".to_string()),
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("founder-outbound:julia-tag-proposal".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: Some(outbound.clone()),
            outbound_anchor: Some("tui-outbound:julia-tag-proposal".to_string()),
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "direct session timeout after 300s")
                .expect("timeout continuation should persist outbound intent");
        assert_eq!(created, None);

        let tasks = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list queue tasks");
        assert!(tasks.is_empty());

        let self_work = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work items");
        assert!(self_work.is_empty());
    }

    #[test]
    fn timeout_blocker_does_not_reuse_current_leased_message_as_continuation() {
        let root = std::env::temp_dir().join(format!(
            "ctox-timeout-followup-current-lease-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("failed to create temp root");
        let current = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Review rework: CRM live login".to_string(),
                prompt: "Fix the currently leased CRM review rework.".to_string(),
                thread_key: "kunstmen-crm-p0-slices".to_string(),
                workspace_root: Some("/tmp/ctox-timeout-current-lease-test".to_string()),
                priority: "high".to_string(),
                suggested_skill: Some("stateful-product-from-scratch".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed current queue task");
        channels::lease_queue_task(&root, &current.message_key, "ctox-service-test")
            .expect("failed to mark current task leased");
        let job = QueuedPrompt {
            prompt: current.prompt.clone(),
            goal: current.title.clone(),
            preview: current.title.clone(),
            source_label: "queue".to_string(),
            suggested_skill: current.suggested_skill.clone(),
            leased_message_keys: vec![current.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some(current.thread_key.clone()),
            workspace_root: current.workspace_root.clone(),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "direct session timeout after 900s")
                .expect("timeout continuation should succeed");

        assert_eq!(created, None);
        let pending = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list pending queue tasks");
        assert!(pending.is_empty());
        let leased = channels::list_queue_tasks(&root, &["leased".to_string()], 10)
            .expect("failed to list leased queue tasks");
        assert_eq!(leased.len(), 1);
        assert_eq!(leased[0].message_key, current.message_key);
    }

    #[test]
    fn timeout_blocker_suppresses_recursive_timeout_continuation() {
        let root = temp_root("ctox-timeout-recursive-continuation");
        let job = QueuedPrompt {
            prompt: "Bearbeite das veroeffentlichte CTOX-Self-Work fuer local.\nTitel: Continue send mail after timeout\nArt: timeout-continuation\nWork-ID: self-work:local:loop\n\nContinue the interrupted task."
                .to_string(),
            goal: "Continue send mail after timeout".to_string(),
            preview: "Continue send mail after timeout".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            leased_message_keys: vec!["queue:system::loop".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("julia-meeting-notetaker-report-20260505".to_string()),
            workspace_root: None,
            ticket_self_work_id: Some("self-work:local:loop".to_string()),
            outbound_email: None,
            outbound_anchor: None,
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "direct session timeout after 300s")
                .expect("recursive timeout suppression should succeed");

        assert_eq!(created, None);
        let tasks = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list pending queue tasks");
        assert!(tasks.is_empty());
    }

    #[test]
    fn fatal_harness_guard_cancels_legacy_timeout_continuation_before_agent_turn() {
        let root = temp_root("ctox-fatal-harness-guard");
        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Continue send mail after timeout".to_string(),
                prompt: "Bearbeite das veroeffentlichte CTOX-Self-Work fuer local.\nTitel: Continue send mail after timeout\nArt: timeout-continuation\nWork-ID: self-work:local:loop\n\nRuntime stop:\ndirect session timeout after 300s"
                    .to_string(),
                thread_key: "julia-meeting-notetaker-report-20260505".to_string(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: Some("follow-up-orchestrator".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed legacy timeout continuation");
        channels::lease_queue_task(&root, &task.message_key, "ctox-service-test")
            .expect("failed to lease legacy timeout continuation");
        let job = QueuedPrompt {
            prompt: task.prompt.clone(),
            goal: task.title.clone(),
            preview: task.title.clone(),
            source_label: "queue".to_string(),
            suggested_skill: task.suggested_skill.clone(),
            leased_message_keys: vec![task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some(task.thread_key.clone()),
            workspace_root: None,
            ticket_self_work_id: Some("self-work:local:loop".to_string()),
            outbound_email: None,
            outbound_anchor: None,
        };
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("state poisoned");
            shared.busy = true;
            shared.active_source_label = Some("queue".to_string());
            shared
                .leased_message_keys_inflight
                .insert(task.message_key.clone());
        }

        let suppressed = maybe_suppress_fatal_harness_prompt_before_execution(&root, &state, &job)
            .expect("fatal harness guard should succeed");

        assert!(suppressed);
        assert_eq!(route_status_for(&root, &task.message_key), "cancelled");
        let open =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue");
        assert!(open.is_empty());
        let shared = state.lock().expect("state poisoned");
        assert!(!shared.busy);
        assert!(shared.active_source_label.is_none());
        assert!(!shared
            .leased_message_keys_inflight
            .contains(&task.message_key));
        assert!(shared
            .recent_events
            .iter()
            .any(|event| event
                .contains("Suppressed fatal harness continuation before model execution")));
        drop(shared);
        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events
            .iter()
            .any(|event| event.mechanism_id == "fatal_harness_loop_guard"));
    }

    #[test]
    fn timeout_without_explicit_auto_retry_blocks_instead_of_requeueing() {
        assert_eq!(failed_worker_route_status(false, true, false), "blocked");
        assert_eq!(failed_worker_route_status(false, true, true), "pending");
        assert_eq!(failed_worker_route_status(true, true, true), "blocked");
        assert_eq!(failed_worker_route_status(false, false, true), "pending");
        assert_eq!(failed_worker_route_status(false, false, false), "failed");
    }

    #[test]
    fn review_rework_is_suppressed_when_same_scope_corrective_task_exists() {
        let root = temp_root("ctox-review-rework-suppressed");
        channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Kunstmen platform homepage reset".to_string(),
                prompt: "Direct corrective work for the Kunstmen homepage.".to_string(),
                thread_key: "kunstmen-operator".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("service-deployment".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed direct corrective task");

        let job = QueuedPrompt {
            prompt: "Bad homepage".to_string(),
            goal: "Repair the Kunstmen homepage".to_string(),
            preview: "Repair the Kunstmen homepage".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            leased_message_keys: vec!["queue-key-1".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-operator".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };
        let outcome = review::ReviewOutcome {
            required: true,
            verdict: review::ReviewVerdict::Fail,
            mission_state: "UNHEALTHY".to_string(),
            score: 0,
            summary: "The homepage is still not a platform.".to_string(),
            report: "report".to_string(),
            reasons: vec!["Reset the IA".to_string()],
            failed_gates: vec!["Mission fit".to_string()],
            semantic_findings: vec!["Homepage still reads like a brochure.".to_string()],
            categorized_findings: Vec::new(),
            open_items: vec!["Introduce clear roster and hire flow.".to_string()],
            evidence: vec!["GET / => static shell".to_string()],
            handoff: None,
            disposition: review::ReviewDisposition::Send,
        };

        let err = enqueue_review_rework(&root, &job, &outcome).expect_err("should suppress");
        assert!(err
            .to_string()
            .contains("superseded by runnable corrective work"));
        let self_work = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert!(self_work.is_empty());
    }

    #[test]
    fn unrelated_active_plan_does_not_swallow_review_rework() {
        let root = temp_root("ctox-review-rework-active-plan");
        plan::ingest_goal(
            &root,
            plan::PlanIngestRequest {
                title: "Unrelated active maintenance plan".to_string(),
                prompt: "Inspect an unrelated service. Then summarize the result.".to_string(),
                thread_key: Some("ops/unrelated".to_string()),
                skill: Some("reliability-ops".to_string()),
                auto_advance: true,
                emit_now: false,
            },
        )
        .expect("failed to seed unrelated active plan");
        assert!(
            plan::has_active_goal_with_pending_step(&root).expect("failed to inspect plan state"),
            "fixture should contain unrelated runnable plan work"
        );

        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: "Repair the Kunstmen CRM tasks workflow.".to_string(),
            goal: "Make CRM tasks usable".to_string(),
            preview: "CRM task workflow".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("stateful-product-from-scratch".to_string()),
            leased_message_keys: vec!["queue-key-1".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-crm-p0-slices".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };
        let outcome = review::ReviewOutcome {
            required: true,
            verdict: review::ReviewVerdict::Fail,
            mission_state: "UNHEALTHY".to_string(),
            score: 6,
            summary:
                "The implementation has evidence but the runtime mission contract remains open."
                    .to_string(),
            report: "report".to_string(),
            reasons: vec!["Runtime mission contract still open".to_string()],
            failed_gates: vec!["Closure readiness".to_string()],
            semantic_findings: vec![
                "Do the remaining rework instead of closing the queue item.".to_string()
            ],
            categorized_findings: Vec::new(),
            open_items: vec!["Persist the missing closure evidence.".to_string()],
            evidence: vec!["review artifact".to_string()],
            handoff: None,
            disposition: review::ReviewDisposition::Send,
        };

        let disposition =
            handle_actionable_completion_review_rejection(&root, &state, &job, &outcome);

        assert!(matches!(
            disposition,
            CompletionReviewDisposition::Hold { .. }
        ));
        let tasks = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].thread_key, "kunstmen-crm-p0-slices");
        assert_eq!(
            tasks[0].suggested_skill.as_deref(),
            Some("stateful-product-from-scratch")
        );
        let self_work = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert_eq!(self_work.len(), 1);
        assert_eq!(self_work[0].kind, "review-rework");
    }

    #[test]
    fn service_self_work_spawn_records_core_parent_child_edges() {
        let root = temp_root("ctox-core-spawn-ledger");
        let task = create_self_work_backed_queue_task(
            &root,
            DurableSelfWorkQueueRequest {
                kind: "mission-follow-up".to_string(),
                title: "Continue mission with modeled spawn".to_string(),
                prompt: "Do the next durable slice.".to_string(),
                thread_key: "queue/modeled-spawn".to_string(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: Some("follow-up-orchestrator".to_string()),
                parent_message_key: None,
                metadata: json!({
                    "dedupe_key": "mission-follow-up:modeled-spawn",
                }),
            },
        )
        .expect("failed to create modeled self-work");
        let work_id = task
            .ticket_self_work_id
            .clone()
            .expect("queue task should point at durable self-work");
        let conn = channels::open_channel_db(&crate::paths::core_db(&root))
            .expect("failed to open runtime db");

        let self_work_edge_count: i64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM ctox_core_spawn_edges
                WHERE accepted = 1
                  AND parent_entity_type = 'Thread'
                  AND parent_entity_id = 'queue/modeled-spawn'
                  AND child_entity_type = 'WorkItem'
                  AND child_entity_id = ?1
                "#,
                params![work_id.as_str()],
                |row| row.get(0),
            )
            .expect("failed to count self-work spawn edges");
        assert_eq!(self_work_edge_count, 1);

        let queue_edge_count: i64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM ctox_core_spawn_edges
                WHERE accepted = 1
                  AND parent_entity_type = 'WorkItem'
                  AND parent_entity_id = ?1
                  AND child_entity_type = 'QueueTask'
                  AND child_entity_id = ?2
                "#,
                params![work_id.as_str(), task.message_key.as_str()],
                |row| row.get(0),
            )
            .expect("failed to count queue spawn edges");
        assert_eq!(queue_edge_count, 1);
    }

    #[test]
    fn review_spawn_budget_blocks_unbounded_self_work_cascade() {
        let root = temp_root("ctox-review-spawn-budget");
        for attempt in 0..review_checkpoint_requeue_block_threshold() {
            create_self_work_backed_queue_task(
                &root,
                DurableSelfWorkQueueRequest {
                    kind: "review-rework".to_string(),
                    title: format!("Review rework attempt {}", attempt + 1),
                    prompt: "External review rejected the previous slice.".to_string(),
                    thread_key: "queue/review-spawn-budget".to_string(),
                    workspace_root: None,
                    priority: "high".to_string(),
                    suggested_skill: Some("follow-up-orchestrator".to_string()),
                    parent_message_key: None,
                    metadata: json!({
                        "dedupe_key": format!("review-rework:queue/review-spawn-budget:{attempt}"),
                    }),
                },
            )
            .expect("budgeted review spawn should be accepted before threshold");
        }

        let err = create_self_work_backed_queue_task(
            &root,
            DurableSelfWorkQueueRequest {
                kind: "review-rework".to_string(),
                title: "Review rework over budget".to_string(),
                prompt: "External review rejected the previous slice again.".to_string(),
                thread_key: "queue/review-spawn-budget".to_string(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: Some("follow-up-orchestrator".to_string()),
                parent_message_key: None,
                metadata: json!({
                    "dedupe_key": "review-rework:queue/review-spawn-budget:over-budget",
                }),
            },
        )
        .expect_err("review spawn over finite budget must be rejected");

        assert!(err.to_string().contains("spawn gate rejected"));
        let items = tickets::list_ticket_self_work_items(&root, Some("local"), Some("blocked"), 10)
            .expect("failed to list blocked self-work");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "review-rework");
    }

    #[test]
    fn review_rejection_resolves_parent_self_work_for_nested_review_rework() {
        let root = temp_root("ctox-review-parent-self-work");
        let parent = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "mission-follow-up".to_string(),
                title: "Continue mission Deliver the Kunstmen homepage reset".to_string(),
                body_text: "Deliver the Kunstmen homepage reset so kunstmen.com reads like a platform for hiring AI employees.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "queue/mission-1",
                    "priority": "high",
                    "skill": "follow-up-orchestrator",
                    "dedupe_key": "mission-follow-up:kunstmen-homepage-reset",
                }),
            },
            false,
        )
        .expect("failed to create parent self-work");
        let parent_task = queue_ticket_self_work_item(&root, &parent)
            .expect("failed to queue parent self-work")
            .expect("parent queue task missing");

        let review_item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "review-rework".to_string(),
                title: "Review rework: Continue mission Deliver the Kunstmen homepage reset (fail)"
                    .to_string(),
                body_text: "External review rejected the last slice.".to_string(),
                state: "queued".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "queue/mission-1",
                    "priority": "high",
                    "skill": "follow-up-orchestrator",
                    "parent_message_key": parent_task.message_key,
                    "dedupe_key": "review-rework:queue/mission-1:fail:test",
                }),
            },
            false,
        )
        .expect("failed to create review self-work");

        let job = QueuedPrompt {
            prompt: "External review rejected the last slice.".to_string(),
            goal: "Repair the Kunstmen homepage".to_string(),
            preview: "Review rework".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            leased_message_keys: vec![parent_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/mission-1".to_string()),
            workspace_root: None,
            ticket_self_work_id: Some(review_item.work_id.clone()),
            outbound_email: None,
            outbound_anchor: None,
        };

        let target = resolve_review_rejection_target_self_work_id(&root, &job);
        assert_eq!(target.as_deref(), Some(parent.work_id.as_str()));
    }

    #[test]
    fn review_rejected_self_work_is_requeued_without_creating_nested_work() {
        let root = temp_root("ctox-review-self-work-requeue");
        let parent = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "mission-follow-up".to_string(),
                title: "Continue mission Deliver the Kunstmen homepage reset".to_string(),
                body_text: "Deliver the Kunstmen homepage reset so kunstmen.com reads like a platform for hiring AI employees.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "queue/mission-1",
                    "priority": "high",
                    "skill": "follow-up-orchestrator",
                    "dedupe_key": "mission-follow-up:kunstmen-homepage-reset",
                }),
            },
            false,
        )
        .expect("failed to create parent self-work");
        let parent_task = queue_ticket_self_work_item(&root, &parent)
            .expect("failed to queue parent self-work")
            .expect("parent queue task missing");
        channels::update_queue_task(
            &root,
            channels::QueueTaskUpdateRequest {
                message_key: parent_task.message_key.clone(),
                route_status: Some("handled".to_string()),
                ..Default::default()
            },
        )
        .expect("failed to mark parent queue task handled");

        let queued = requeue_review_rejected_self_work(
            &root,
            &parent.work_id,
            "The homepage still does not read like an AI hiring platform.",
        )
        .expect("failed to requeue parent self-work")
        .expect("expected a new queue task");

        let reloaded = tickets::load_ticket_self_work_item(&root, &parent.work_id)
            .expect("failed to reload self-work")
            .expect("missing self-work");
        assert_eq!(reloaded.state, "queued");
        assert_eq!(queued.thread_key, "queue/mission-1");
        assert!(queued
            .title
            .contains("Continue mission Deliver the Kunstmen homepage reset"));

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn requeue_reuses_existing_runnable_self_work_slice() {
        let root = temp_root("ctox-self-work-runnable-dedupe");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "mission-follow-up".to_string(),
                title: "Continue mission without duplicating queue work".to_string(),
                body_text: "Keep using the existing runnable slice for this durable work item."
                    .to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "queue/mission-dedupe",
                    "priority": "high",
                    "skill": "follow-up-orchestrator",
                    "dedupe_key": "mission-follow-up:dedupe-test",
                }),
            },
            false,
        )
        .expect("failed to create self-work");
        let first = queue_ticket_self_work_item(&root, &item)
            .expect("failed to queue self-work")
            .expect("expected initial queue task");

        let reused = requeue_review_rejected_self_work(
            &root,
            &item.work_id,
            "Review rejected the slice while the original queue task is still runnable.",
        )
        .expect("failed to requeue self-work")
        .expect("expected existing runnable queue task to be reused");

        assert_eq!(reused.message_key, first.message_key);
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        let matching = tasks
            .iter()
            .filter(|task| task.ticket_self_work_id.as_deref() == Some(item.work_id.as_str()))
            .count();
        assert_eq!(matching, 1);
    }

    #[test]
    fn generic_review_requeue_blocks_after_finite_checkpoint_threshold() {
        let root = temp_root("ctox-generic-review-requeue-threshold");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "mission-follow-up".to_string(),
                title: "Continue mission with bounded review requeues".to_string(),
                body_text: "Repeated review failures must not loop forever.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "queue/bounded-review",
                    "priority": "high",
                    "skill": "follow-up-orchestrator",
                    "dedupe_key": "mission-follow-up:bounded-review",
                }),
            },
            false,
        )
        .expect("failed to create self-work");

        let first = requeue_review_rejected_self_work(&root, &item.work_id, "first rejection")
            .expect("first requeue should succeed")
            .expect("first requeue should create queue task");
        channels::update_queue_task(
            &root,
            channels::QueueTaskUpdateRequest {
                message_key: first.message_key,
                route_status: Some("handled".to_string()),
                ..Default::default()
            },
        )
        .expect("failed to mark first task handled");

        let second = requeue_review_rejected_self_work(&root, &item.work_id, "second rejection")
            .expect("second requeue should succeed")
            .expect("second requeue should create queue task");

        let blocked = requeue_review_rejected_self_work(&root, &item.work_id, "third rejection")
            .expect("threshold block should be handled");
        assert!(blocked.is_none());

        let reloaded = tickets::load_ticket_self_work_item(&root, &item.work_id)
            .expect("failed to reload self-work")
            .expect("missing self-work");
        assert_eq!(reloaded.state, "blocked");

        let blocked_tasks = channels::list_queue_tasks(&root, &["blocked".to_string()], 10)
            .expect("failed to list blocked queue tasks");
        assert!(blocked_tasks
            .iter()
            .any(|task| task.message_key == second.message_key));
    }

    #[test]
    fn runtime_api_retry_review_rejection_blocks_without_requeue() {
        let root = temp_root("ctox-runtime-api-retry-review-block");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: RUNTIME_API_RETRY_KIND.to_string(),
                title: "Retry Jill reviewed send after API failure".to_string(),
                body_text: "Continue the exact reviewed-send turn after a transient API failure."
                    .to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "queue/runtime-api-retry",
                    "priority": "urgent",
                    "skill": "follow-up-orchestrator",
                    "dedupe_key": "runtime-api-retry:jill-reviewed-send",
                }),
            },
            false,
        )
        .expect("failed to create runtime retry self-work");
        let first = queue_ticket_self_work_item(&root, &item)
            .expect("failed to queue runtime retry self-work")
            .expect("expected initial runtime retry queue task");

        let requeued = requeue_review_rejected_self_work(
            &root,
            &item.work_id,
            "NO-SEND: the worker produced an internal status update instead of the reviewed send.",
        )
        .expect("runtime retry review rejection should be handled");
        assert!(requeued.is_none());

        let reloaded = tickets::load_ticket_self_work_item(&root, &item.work_id)
            .expect("failed to reload self-work")
            .expect("missing self-work");
        assert_eq!(reloaded.state, "blocked");

        let pending =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list pending queue tasks");
        assert!(pending.is_empty());

        let blocked = channels::list_queue_tasks(&root, &["blocked".to_string()], 10)
            .expect("failed to list blocked queue tasks");
        assert!(blocked
            .iter()
            .any(|task| task.message_key == first.message_key
                && task
                    .status_note
                    .as_deref()
                    .unwrap_or("")
                    .contains("darf nach einem Review-Reject nicht automatisch erneut starten")));
    }

    #[test]
    fn review_gate_worst_case_model_has_strictly_finite_variant() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum AbstractRoute {
            RewriteOnly,
            SpawnedRework,
            SameWorkCheckpoint,
            Terminal,
        }

        fn next(
            route: AbstractRoute,
            rewrite_budget: usize,
            requeue_budget: usize,
        ) -> AbstractRoute {
            match route {
                AbstractRoute::RewriteOnly if rewrite_budget > 0 => AbstractRoute::RewriteOnly,
                AbstractRoute::RewriteOnly => AbstractRoute::Terminal,
                AbstractRoute::SpawnedRework => AbstractRoute::SameWorkCheckpoint,
                AbstractRoute::SameWorkCheckpoint if requeue_budget > 0 => {
                    AbstractRoute::SameWorkCheckpoint
                }
                AbstractRoute::SameWorkCheckpoint | AbstractRoute::Terminal => {
                    AbstractRoute::Terminal
                }
            }
        }
        fn route_weight(route: AbstractRoute) -> usize {
            match route {
                AbstractRoute::SpawnedRework => 2,
                AbstractRoute::RewriteOnly | AbstractRoute::SameWorkCheckpoint => 1,
                AbstractRoute::Terminal => 0,
            }
        }

        for start in [
            AbstractRoute::RewriteOnly,
            AbstractRoute::SpawnedRework,
            AbstractRoute::SameWorkCheckpoint,
            AbstractRoute::Terminal,
        ] {
            let mut route = start;
            let mut rewrite_budget = mission_rewrite_failure_threshold().max(0) as usize;
            let mut requeue_budget = review_checkpoint_requeue_block_threshold();
            let mut variant = rewrite_budget + requeue_budget + route_weight(route);

            for _ in 0..16 {
                let previous_variant = variant;
                route = next(route, rewrite_budget, requeue_budget);
                match route {
                    AbstractRoute::RewriteOnly => {
                        rewrite_budget = rewrite_budget.saturating_sub(1);
                    }
                    AbstractRoute::SameWorkCheckpoint => {
                        requeue_budget = requeue_budget.saturating_sub(1);
                    }
                    AbstractRoute::SpawnedRework | AbstractRoute::Terminal => {}
                }
                variant = rewrite_budget + requeue_budget + route_weight(route);
                assert!(
                    route == AbstractRoute::Terminal || variant < previous_variant,
                    "route={route:?} variant={variant} previous={previous_variant}"
                );
                if route == AbstractRoute::Terminal {
                    break;
                }
            }
            assert_eq!(
                route,
                AbstractRoute::Terminal,
                "{start:?} did not terminate"
            );
        }
    }

    #[test]
    fn published_review_rework_is_blocked_when_same_scope_corrective_task_exists() {
        let root = temp_root("ctox-review-rework-route-suppressed");
        channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Kunstmen platform homepage reset".to_string(),
                prompt: "Direct corrective work for the Kunstmen homepage.".to_string(),
                thread_key: "kunstmen-operator".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("service-deployment".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed direct corrective task");

        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "review-rework".to_string(),
                title: "Review rework: Kunstmen homepage".to_string(),
                body_text: "Repair the failed homepage review.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "kunstmen-operator",
                    "workspace_root": "/home/ubuntu/workspace/kunstmen",
                    "priority": "high",
                    "skill": "follow-up-orchestrator",
                    "dedupe_key": "review-rework:kunstmen-operator:fail:test",
                }),
            },
            true,
        )
        .expect("failed to create review rework");
        tickets::assign_ticket_self_work_item(&root, &item.work_id, "self", "ctox", None)
            .expect("failed to assign self-work");

        let state = Arc::new(Mutex::new(SharedState::default()));
        route_assigned_ticket_self_work(&root, &state).expect("routing should succeed");

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Kunstmen platform homepage reset");

        let closed = tickets::list_ticket_self_work_items(&root, Some("local"), Some("closed"), 10)
            .expect("failed to list closed self-work");
        assert!(closed.iter().any(|entry| entry.work_id == item.work_id));
    }

    #[test]
    fn active_superseded_review_rework_is_skipped_before_turn_execution() {
        let root = temp_root("ctox-review-rework-active-suppressed");
        channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Kunstmen platform homepage reset".to_string(),
                prompt: "Direct corrective work for the Kunstmen homepage.".to_string(),
                thread_key: "kunstmen-operator".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("service-deployment".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed direct corrective task");

        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "review-rework".to_string(),
                title: "Review rework: Kunstmen homepage".to_string(),
                body_text: "Repair the failed homepage review.".to_string(),
                state: "queued".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "kunstmen-operator",
                    "workspace_root": "/home/ubuntu/workspace/kunstmen",
                    "priority": "high",
                    "skill": "follow-up-orchestrator",
                    "dedupe_key": "review-rework:kunstmen-operator:fail:test",
                }),
            },
            true,
        )
        .expect("failed to create review rework");
        tickets::assign_ticket_self_work_item(&root, &item.work_id, "self", "ctox", None)
            .expect("failed to assign self-work");

        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("service state poisoned");
            shared.busy = true;
            shared.current_goal_preview = Some("Review rework".to_string());
            shared.active_source_label = Some("queue".to_string());
            track_leased_keys_locked(&mut shared, &["queue-key-1".to_string()], &[]);
        }
        let job = QueuedPrompt {
            prompt: "Repair the failed homepage review.".to_string(),
            goal: "Repair the Kunstmen homepage".to_string(),
            preview: "Review rework".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            leased_message_keys: vec!["queue-key-1".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-operator".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: Some(item.work_id.clone()),
            outbound_email: None,
            outbound_anchor: None,
        };

        let skipped = maybe_skip_superseded_self_work_prompt(&root, &state, &job)
            .expect("skip check should succeed");
        assert!(skipped);

        let closed = tickets::list_ticket_self_work_items(&root, Some("local"), Some("closed"), 10)
            .expect("failed to list closed self-work");
        assert!(closed.iter().any(|entry| entry.work_id == item.work_id));

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Kunstmen platform homepage reset");
    }

    #[test]
    fn published_watchdog_mission_follow_up_is_closed_when_direct_reset_exists() {
        let root = temp_root("ctox-mission-follow-up-route-suppressed");
        channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Kunstmen platform homepage reset".to_string(),
                prompt: "Direct corrective work for the Kunstmen homepage.".to_string(),
                thread_key: "kunstmen-operator".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("service-deployment".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed direct corrective task");

        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "mission-follow-up".to_string(),
                title: "Continue mission Monitor inbound non-queue channels for explicit owner approval".to_string(),
                body_text: "Mission continuity watchdog: the mission was idle for 59s.\n\nMission: Monitor inbound non-queue channels for explicit owner approval/access-grant confirmation for Vercel team/project access for kunstmen.com.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "kunstmen-operator",
                    "workspace_root": "/home/ubuntu/workspace/kunstmen",
                    "priority": "high",
                    "skill": "follow-up-orchestrator",
                    "dedupe_key": "mission-watchdog:kunstmen-operator",
                }),
            },
            true,
        )
        .expect("failed to create watchdog follow-up");
        tickets::assign_ticket_self_work_item(&root, &item.work_id, "self", "ctox", None)
            .expect("failed to assign self-work");

        let state = Arc::new(Mutex::new(SharedState::default()));
        route_assigned_ticket_self_work(&root, &state).expect("routing should succeed");

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Kunstmen platform homepage reset");

        let closed = tickets::list_ticket_self_work_items(&root, Some("local"), Some("closed"), 10)
            .expect("failed to list closed self-work");
        assert!(closed.iter().any(|entry| entry.work_id == item.work_id));
    }

    #[test]
    fn active_watchdog_mission_follow_up_is_skipped_before_turn_execution() {
        let root = temp_root("ctox-mission-follow-up-active-suppressed");
        channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Kunstmen platform homepage reset".to_string(),
                prompt: "Direct corrective work for the Kunstmen homepage.".to_string(),
                thread_key: "kunstmen-operator".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("service-deployment".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed direct corrective task");

        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "mission-follow-up".to_string(),
                title: "Continue mission Monitor inbound non-queue channels for explicit owner approval".to_string(),
                body_text: "Mission continuity watchdog: the mission was idle for 59s.\n\nMission: Monitor inbound non-queue channels for explicit owner approval/access-grant confirmation for Vercel team/project access for kunstmen.com.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "kunstmen-operator",
                    "workspace_root": "/home/ubuntu/workspace/kunstmen",
                    "priority": "high",
                    "skill": "follow-up-orchestrator",
                    "dedupe_key": "mission-watchdog:kunstmen-operator",
                }),
            },
            true,
        )
        .expect("failed to create watchdog follow-up");
        tickets::assign_ticket_self_work_item(&root, &item.work_id, "self", "ctox", None)
            .expect("failed to assign self-work");

        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = lock_shared_state(&state);
            shared.busy = true;
            shared.current_goal_preview = Some("Continue mission Monitor inbound ...".to_string());
            shared.active_source_label = Some("queue".to_string());
            shared
                .leased_message_keys_inflight
                .insert("queue-key-1".to_string());
        }
        let job = QueuedPrompt {
            prompt: "Monitor inbound approval".to_string(),
            goal: "Continue mission Monitor inbound non-queue channels for explicit owner approval"
                .to_string(),
            preview:
                "Continue mission Monitor inbound non-queue channels for explicit owner approval"
                    .to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            leased_message_keys: vec!["queue-key-1".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-operator".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: Some(item.work_id.clone()),
            outbound_email: None,
            outbound_anchor: None,
        };

        let skipped = maybe_skip_superseded_self_work_prompt(&root, &state, &job)
            .expect("skip evaluation should succeed");
        assert!(skipped);

        let closed = tickets::list_ticket_self_work_items(&root, Some("local"), Some("closed"), 10)
            .expect("failed to list closed self-work");
        assert!(closed.iter().any(|entry| entry.work_id == item.work_id));

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Kunstmen platform homepage reset");
    }

    #[test]
    fn owner_visible_platform_reset_is_redirected_into_first_expertise_pass() {
        let root = temp_root("ctox-platform-pass-reroute");
        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Kunstmen platform homepage reset".to_string(),
                prompt: "Reset kunstmen.com so it behaves like a platform.".to_string(),
                thread_key: "kunstmen-supervisor".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("follow-up-orchestrator".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed active queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = lock_shared_state(&state);
            shared.busy = true;
            shared.current_goal_preview = Some("Kunstmen platform homepage reset".to_string());
            shared.active_source_label = Some("queue".to_string());
            track_leased_keys_locked(
                &mut shared,
                std::slice::from_ref(&queue_task.message_key),
                &[],
            );
        }
        let job = QueuedPrompt {
            prompt: "Reset kunstmen.com so it behaves like a platform for hiring AI employees."
                .to_string(),
            goal: "Kunstmen platform homepage reset".to_string(),
            preview: "Kunstmen platform homepage reset".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-supervisor".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_platform_work_to_expertise_passes(&root, &state, &job)
            .expect("platform pass reroute should succeed");
        assert!(redirected);

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, PLATFORM_EXPERTISE_KIND);
        assert_eq!(
            platform_expertise_pass_kind(&items[0]).as_deref(),
            Some("platform-ia")
        );
        assert_eq!(
            items[0].suggested_skill.as_deref(),
            Some("plan-orchestrator")
        );
    }

    #[test]
    fn missing_strategy_reroutes_owner_visible_work_into_strategic_direction_pass() {
        let root = temp_root("ctox-strategy-reroute");
        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Kunstmen platform homepage reset".to_string(),
                prompt: "Reset kunstmen.com so it behaves like a platform.".to_string(),
                thread_key: "kunstmen-supervisor".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("follow-up-orchestrator".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed active queue task");
        let stale_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Repair Stripe runtime and rerun Kunstmen live gates".to_string(),
                prompt: "Legacy Stripe recheck that should be superseded by strategy setup."
                    .to_string(),
                thread_key: "kunstmen-supervisor".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "high".to_string(),
                suggested_skill: Some("service-deployment".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed stale competing queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = lock_shared_state(&state);
            shared.busy = true;
            shared.current_goal_preview = Some("Kunstmen platform homepage reset".to_string());
            shared.active_source_label = Some("queue".to_string());
            track_leased_keys_locked(
                &mut shared,
                std::slice::from_ref(&queue_task.message_key),
                &[],
            );
        }
        let job = QueuedPrompt {
            prompt: "Reset kunstmen.com so it behaves like a platform for hiring AI employees."
                .to_string(),
            goal: "Kunstmen platform homepage reset".to_string(),
            preview: "Kunstmen platform homepage reset".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-supervisor".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("strategy reroute should succeed");
        assert!(redirected);

        let stale = channels::load_queue_task(&root, &stale_task.message_key)
            .expect("failed to reload stale queue task")
            .expect("missing stale queue task");
        assert_eq!(stale.route_status, "cancelled");

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Strategic direction setup");

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, STRATEGIC_DIRECTION_KIND);
        assert_eq!(
            items[0].suggested_skill.as_deref(),
            Some("follow-up-orchestrator")
        );
        assert!(items[0]
            .body_text
            .contains("Use `ctox strategy show --conversation-id"));
        assert!(items[0]
            .body_text
            .contains("--thread-key kunstmen-supervisor"));
    }

    #[test]
    fn scoped_stateful_product_execution_does_not_reroute_to_strategy_setup() {
        let root = temp_root("ctox-scoped-stateful-product-no-strategy-reroute");
        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "CRM P0 slice: ship tasks workflow under /internal/crm".to_string(),
                prompt: "Work only in /home/ubuntu/workspace/kunstmen. Next smallest coherent slice: make Tasks a real founder-usable workflow under /internal/crm with create/edit/delete/status changes linked to CRM records."
                    .to_string(),
                thread_key: "kunstmen-crm-p0".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("stateful-product-from-scratch".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed scoped CRM queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: queue_task.prompt.clone(),
            goal: queue_task.title.clone(),
            preview: queue_task.title.clone(),
            source_label: "queue".to_string(),
            suggested_skill: Some("stateful-product-from-scratch".to_string()),
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-crm-p0".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("strategy evaluation should succeed");
        assert!(!redirected);

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].title,
            "CRM P0 slice: ship tasks workflow under /internal/crm"
        );

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert!(items.is_empty());
    }

    #[test]
    fn terminal_bench_controller_does_not_reroute_to_strategy_setup() {
        let root = temp_root("ctox-terminal-bench-no-strategy-reroute");
        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Terminal-Bench 2 controller Qwen3.6 128k".to_string(),
                prompt: "You are CTOX running the Terminal-Bench 2 evaluation project.\n\
Use the configured local harness model through CTOX local IPC only.\n\
REQUIRED OUTPUT ARTIFACTS\n\
Write these exact files under RUN_DIR=/home/metricspace/CTOX/runtime/terminal-bench-2/runs/run-1:\n\
- controller.json\n- ticket-map.jsonl\n- run-log.md\n- results.jsonl\n- summary.md\n\
Start now by using shell/tools to create RUN_DIR artifacts and verify runtime/context."
                    .to_string(),
                thread_key: "tb2-qwen36-128k-controller".to_string(),
                workspace_root: Some("/home/metricspace".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed Terminal-Bench queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: queue_task.prompt.clone(),
            goal: queue_task.title.clone(),
            preview: queue_task.title.clone(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tb2-qwen36-128k-controller".to_string()),
            workspace_root: Some("/home/metricspace".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("strategy evaluation should succeed");
        assert!(!redirected);

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Terminal-Bench 2 controller Qwen3.6 128k");

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert!(items.is_empty());
    }

    #[test]
    fn terminal_bench_prep_ticket_without_workspace_root_does_not_reroute_to_strategy_setup() {
        let root = temp_root("ctox-terminal-bench-prep-no-workspace-no-strategy-reroute");
        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "prep-priority-plan: choose first easy Terminal-Bench tasks".to_string(),
                prompt: "Preparation ticket for the Terminal-Bench 2 controller.\n\
Research public Terminal-Bench references and leaderboard/model result lists for task selection only; do not read solutions.\n\
Pick initial benchmark tasks that other harnesses/models are known to solve, then update ticket-map.jsonl, run-queue.jsonl, knowledge.md, and logbook.md in the current RUN_DIR.\n\
Use Harbor/Terminal-Bench runner evidence and keep the work bounded to benchmark preparation."
                    .to_string(),
                thread_key: "queue/prep-priority-plan-choose-first-easy-1be52fca49b0"
                    .to_string(),
                workspace_root: None,
                priority: "urgent".to_string(),
                suggested_skill: Some("benchmark-controller".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed Terminal-Bench prep queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: queue_task.prompt.clone(),
            goal: queue_task.title.clone(),
            preview: queue_task.title.clone(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/prep-priority-plan-choose-first-easy-1be52fca49b0".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert!(is_bounded_benchmark_or_runtime_execution_job(&job));
        assert!(!is_owner_visible_strategic_job(&job));
        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("strategy evaluation should succeed");
        assert!(!redirected);

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].title,
            "prep-priority-plan: choose first easy Terminal-Bench tasks"
        );

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert!(items.is_empty());
    }

    #[test]
    fn terminal_bench_controller_with_artifact_contract_does_not_reroute_to_strategy_setup() {
        let root = temp_root("ctox-terminal-bench-artifact-contract-no-strategy-reroute");
        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Terminal-Bench 2 controller Qwen3.6 128k clean".to_string(),
                prompt: "You are CTOX running as the Terminal-Bench 2 benchmark controller.\n\
RUNTIME CONTRACT\n\
- The required context window is 128k tokens / 131072 tokens. Verify this from CTOX status/runtime evidence.\n\
- Inference must stay on local CTOX IPC/native backend.\n\
DURABLE ARTIFACT CONTRACT\n\
Create these five files immediately, before open-ended discovery or research.\n\
1. /home/metricspace/CTOX/runtime/terminal-bench-2/runs/run-1/controller.json\n\
2. /home/metricspace/CTOX/runtime/terminal-bench-2/runs/run-1/ticket-map.jsonl\n\
3. /home/metricspace/CTOX/runtime/terminal-bench-2/runs/run-1/run-log.md\n\
4. /home/metricspace/CTOX/runtime/terminal-bench-2/runs/run-1/results.jsonl\n\
5. /home/metricspace/CTOX/runtime/terminal-bench-2/runs/run-1/summary.md"
                    .to_string(),
                thread_key: "tb2-qwen36-128k-controller-artifact-contract".to_string(),
                workspace_root: Some("/home/metricspace".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed Terminal-Bench queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: queue_task.prompt.clone(),
            goal: queue_task.title.clone(),
            preview: queue_task.title.clone(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tb2-qwen36-128k-controller-artifact-contract".to_string()),
            workspace_root: Some("/home/metricspace".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("strategy evaluation should succeed");
        assert!(!redirected);

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].title,
            "Terminal-Bench 2 controller Qwen3.6 128k clean"
        );

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert!(items.is_empty());
    }

    #[test]
    fn terminal_bench_controller_with_required_artifacts_does_not_reroute_or_infer_extra_files() {
        let root = temp_root("ctox-terminal-bench-required-artifacts-no-strategy-reroute");
        let run_dir = "/home/metricspace/CTOX/runtime/terminal-bench-2/runs/run-required-artifacts";
        let prompt = format!(
            "You are CTOX running a Terminal-Bench 2 evaluation controller on this 4xGPU host.\n\n\
Hard runtime facts you must verify before doing benchmark work:\n\
- CTOX release must be branch-main-20260506T031528Z or newer.\n\
- Harness model must be Qwen/Qwen3.6-35B-A3B.\n\
- Inference source must be local.\n\
- Local runtime must be ggml.\n\
- Effective context must be 131072 tokens (128k). Any smaller context is invalid and must be fixed before continuing.\n\
- The Qwen process must use --ctx 131072 and CUDA GPUs 0,1,2,3.\n\
- Do not use an HTTP inference path.\n\n\
Your mission is to run Terminal-Bench 2 through Harbor as an honest benchmark controller.\n\
Public references to use:\n\
- https://registry.hub.databricks.com/environments/terminal-bench:2.0\n\
- https://evalscope.readthedocs.io/en/latest/third_party/terminal_bench.html\n\
- https://www.tbench.ai/leaderboard\n\
- https://github.com/laude-institute/terminal-bench\n\n\
Required artifacts. You must create and maintain exactly these durable files in this run directory:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/run-log.md\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/results.json\n\n\
Initial artifact requirements:\n\
- controller.json must include the phase.\n\
- ticket-map.jsonl must include preparation tickets.\n\n\
Use shell tools to create or update these files."
        );
        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Terminal-Bench 2 controller Qwen3.6 128k c1ad584".to_string(),
                prompt: prompt.clone(),
                thread_key: "queue/terminal-bench-2-controller-qwen3-6".to_string(),
                workspace_root: Some("/home/metricspace/CTOX/runtime/terminal-bench-2".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed Terminal-Bench queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: queue_task.prompt.clone(),
            goal: queue_task.title.clone(),
            preview: queue_task.title.clone(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/terminal-bench-2-controller-qwen3-6".to_string()),
            workspace_root: Some("/home/metricspace/CTOX/runtime/terminal-bench-2".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("strategy evaluation should succeed");
        assert!(!redirected);

        let paths = expected_outcome_artifacts_for_job(&job)
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::WorkspaceFile)
            .map(|artifact| artifact.primary_key.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                format!("{run_dir}/controller.json"),
                format!("{run_dir}/ticket-map.jsonl"),
                format!("{run_dir}/run-log.md"),
                format!("{run_dir}/knowledge.md"),
                format!("{run_dir}/results.json"),
            ]
        );
    }

    #[test]
    fn terminal_bench_artifact_parser_ignores_openrouter_model_ids() {
        let run_dir = "/home/metricspace/ctox/runtime/terminal-bench-2/runs/run-deepseek-v4-flash";
        let prompt = format!(
            "Terminal-Bench controller artifact contract:\n\
- {run_dir}/controller.json\n\
- {run_dir}/logbook.md\n\n\
Current RUN_DIR and workspace scope: {run_dir}; active model deepseek/deepseek-v4-flash via OpenRouter.\n\
Preserve and update controller.json and logbook.md."
        );
        let job = QueuedPrompt {
            prompt,
            goal: "Terminal-Bench 2 controller DeepSeek flash".to_string(),
            preview: "Terminal-Bench 2 controller DeepSeek flash".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::parent".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("terminal-bench-2/deepseek-v4-flash/controller".to_string()),
            workspace_root: Some(run_dir.to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let paths = declared_workspace_file_artifacts_for_job(&job);

        assert_eq!(
            paths,
            vec![
                format!("{run_dir}/controller.json"),
                format!("{run_dir}/logbook.md"),
            ]
        );
        assert!(!paths
            .iter()
            .any(|path| path.starts_with("/deepseek-v4-flash/")));
    }

    #[test]
    fn terminal_bench_review_hold_generates_actionable_continuation_feedback() {
        let run_dir = "/home/metricspace/CTOX/runtime/terminal-bench-2/runs/run-hold-feedback";
        let prompt = format!(
            "You are CTOX running a Terminal-Bench 2 evaluation controller on this 4xGPU host.\n\n\
Required artifacts. You must create and maintain exactly these durable files in this run directory:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/run-log.md\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/results.json\n\n\
Use shell tools to create or update these files through Harbor."
        );
        let job = QueuedPrompt {
            prompt,
            goal: "Terminal-Bench 2 controller Qwen3.6 128k".to_string(),
            preview: "Terminal-Bench 2 controller Qwen3.6 128k".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:system::tb2".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/tb2-qwen36-128k".to_string()),
            workspace_root: Some("/home/metricspace/CTOX/runtime/terminal-bench-2".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert!(is_terminal_bench_controller_artifact_job(&job));

        let feedback = terminal_bench_controller_hold_feedback_prompt(
            &job,
            "completion review leg did not produce a verdict within 300s",
        );
        assert!(feedback.contains("HARNESS FEEDBACK"));
        assert!(feedback.contains("not accepted as benchmark completion"));
        assert!(feedback.contains("Do not recreate them from scratch"));
        assert!(feedback.contains("controller.phase=preparation"));
        assert!(feedback.contains("results.json with zero real task statuses"));
        assert!(!feedback.contains("same five durable files"));
        assert!(!feedback.contains("Read the existing controller.json, ticket-map.jsonl, run-log.md, knowledge.md, and results.json first"));
        assert!(feedback.contains("active harness model/provider"));
        assert!(feedback.contains("131072 tokens"));
        assert!(feedback.contains("API-backed"));
        assert!(feedback.contains("Verify Harbor"));
        assert!(feedback.contains(&format!("{run_dir}/controller.json")));
        assert!(feedback.contains(&format!("{run_dir}/results.json")));
    }

    #[test]
    fn unavailable_review_does_not_requeue_generic_artifact_job() {
        let job = QueuedPrompt {
            prompt: "RUN_DIR=\"/tmp/ctox-smoke\". Initialisiere die Datei required-smoke.json."
                .to_string(),
            goal: "smoke artifact".to_string(),
            preview: "smoke artifact".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:smoke-artifact".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert!(!is_terminal_bench_controller_artifact_job(&job));
        assert!(matches!(
            completion_review_unavailable_disposition(
                &job,
                "completion review leg did not produce a verdict within 300s"
            ),
            CompletionReviewDisposition::None
        ));
    }

    #[test]
    fn unavailable_review_still_holds_terminal_bench_controller() {
        let run_dir = "/home/metricspace/CTOX/runtime/terminal-bench-2/runs/run-hold-feedback";
        let job = QueuedPrompt {
            prompt: format!(
                "Terminal-Bench 2 controller via Harbor.\n\n\
Required artifacts. You must create and maintain exactly these durable files in this run directory:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/run-log.md\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/results.json\n"
            ),
            goal: "Terminal-Bench 2 controller Qwen3.6 128k".to_string(),
            preview: "Terminal-Bench 2 controller Qwen3.6 128k".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:system::tb2".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/tb2-qwen36-128k".to_string()),
            workspace_root: Some("/home/metricspace/CTOX/runtime/terminal-bench-2".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert!(is_terminal_bench_controller_artifact_job(&job));
        assert!(matches!(
            completion_review_unavailable_disposition(
                &job,
                "completion review leg did not produce a verdict within 300s"
            ),
            CompletionReviewDisposition::Hold { .. }
        ));
    }

    #[test]
    fn proactive_founder_outbound_does_not_reroute_to_strategy_setup() {
        let root = temp_root("ctox-proactive-founder-outbound-no-strategy-reroute");
        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: "Write the honest Kunstmen CRM interim update for the founders.".to_string(),
            goal: "Kunstmen CRM founder interim mail".to_string(),
            preview: "Founder outbound mail about Kunstmen CRM".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("chat-outbound".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: Some(channels::FounderOutboundAction {
                account_key: "email:cto1@example.test".to_string(),
                thread_key: "chat-outbound".to_string(),
                subject: "Kunstmen CRM: ehrlicher Zwischenstand".to_string(),
                to: vec!["founder@example.test".to_string()],
                cc: Vec::new(),
                attachments: Vec::new(),
            }),
            outbound_anchor: Some("tui-outbound:test".to_string()),
        };

        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("proactive founder outbound should not fail reroute check");
        assert!(!redirected);
    }

    #[test]
    fn strategic_direction_pass_is_not_rerouted_into_platform_passes() {
        let root = temp_root("ctox-strategy-pass-no-platform-reroute");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: STRATEGIC_DIRECTION_KIND.to_string(),
                title: "Strategic direction setup".to_string(),
                body_text: "Establish Vision and Mission before continuing Kunstmen platform work."
                    .to_string(),
                state: "queued".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "kunstmen-supervisor",
                    "workspace_root": "/home/ubuntu/workspace/kunstmen",
                    "priority": "urgent",
                    "skill": "plan-orchestrator",
                    "resume_prompt": "Reset kunstmen.com so it behaves like a platform.",
                }),
            },
            false,
        )
        .expect("failed to create strategic direction self-work");
        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: "Before further strategic or owner-visible execution, establish canonical runtime direction in SQLite.\n\nAfter direction is canonical, the deferred execution target is:\nReset kunstmen.com so it behaves like a platform for hiring AI employees.".to_string(),
            goal: "Strategic direction setup".to_string(),
            preview: "Strategic direction setup".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("plan-orchestrator".to_string()),
            leased_message_keys: vec!["queue:system::strategy".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-supervisor".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: Some(item.work_id.clone()),
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_platform_work_to_expertise_passes(&root, &state, &job)
            .expect("platform reroute check should succeed");
        assert!(!redirected);

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, STRATEGIC_DIRECTION_KIND);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn internal_harness_smoke_does_not_reroute_to_strategy_setup() {
        let root = temp_root("ctox-internal-harness-smoke-no-strategy-reroute");
        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: "Interner CTOX-Harness-Smoke-Test. Keine externe Kommunikation. Pruefe Process-Mining-Selbstdiagnose und Founder review warnings.".to_string(),
            goal: "Process-mining harness smoke".to_string(),
            preview: "Codex harness smoke: process mining and no external communication".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:system::smoke".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("codex/harness-live-smoke-20260426".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("internal harness smoke should not fail reroute check");
        assert!(!redirected);
    }

    #[test]
    fn founder_email_sqlite_lock_is_retryable() {
        let job = QueuedPrompt {
            prompt: "Reply to founder".to_string(),
            goal: "Founder communication".to_string(),
            preview: "Founder mail".to_string(),
            source_label: "email:founder".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["email:cto1@metric-space.ai::INBOX::95".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("<founder-thread@example.com>".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert!(founder_email_worker_error_is_retryable(
            &job,
            "database is locked"
        ));
    }

    #[test]
    fn non_founder_sqlite_lock_is_not_founder_retryable() {
        let job = QueuedPrompt {
            prompt: "Run platform work".to_string(),
            goal: "Platform work".to_string(),
            preview: "Queue task".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:system::abc".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-supervisor".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert!(!founder_email_worker_error_is_retryable(
            &job,
            "database is locked"
        ));
    }

    #[test]
    fn runtime_rate_limit_error_is_retryable_api_failure() {
        let error = "stream disconnected before completion: HTTP status 429 Too Many Requests";
        assert_eq!(
            turn_loop::hard_runtime_blocker_retry_cooldown_secs(error),
            Some(300)
        );
        assert!(runtime_error_is_transient_api_failure(error));
    }

    #[test]
    fn no_assistant_message_error_is_retryable_runtime_failure() {
        let error = "turn completed without assistant message";
        assert_eq!(
            turn_loop::hard_runtime_blocker_retry_cooldown_secs(error),
            Some(60)
        );
        assert!(runtime_error_is_transient_api_failure(error));
        assert_eq!(
            classify_agent_failure(error),
            crate::lcm::AgentOutcome::ExecutionError
        );
    }

    #[test]
    fn no_assistant_retry_prompt_explains_missing_final_message() {
        let job = QueuedPrompt {
            prompt: "Create and verify /tmp/result.txt.".to_string(),
            goal: "Create and verify /tmp/result.txt.".to_string(),
            preview: "Create result artifact".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("smoke".to_string()),
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let prompt = render_runtime_retry_prompt(&job, "turn completed without assistant message");

        assert!(prompt.contains("HARNESS FEEDBACK"));
        assert!(prompt.contains("without producing the required final assistant message"));
        assert!(prompt.contains("continue after the tool phase"));
        assert!(prompt.contains("EXIT GATE"));
    }

    #[test]
    fn terminal_bench_preflight_violation_is_retryable_with_specific_feedback() {
        let error = "terminal-bench preflight violation: the first shell command did not create and verify the required current-run artifacts.";
        assert_eq!(
            turn_loop::hard_runtime_blocker_retry_cooldown_secs(error),
            Some(60)
        );
        assert!(runtime_error_is_transient_api_failure(error));
        let job = QueuedPrompt {
            prompt: "Only required durable files for this controller turn:\n- /tmp/tb/controller.json\n- /tmp/tb/ticket-map.jsonl\n- /tmp/tb/preparation-tickets.jsonl".to_string(),
            goal: "Terminal-Bench controller preflight".to_string(),
            preview: "Terminal-Bench controller preflight".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tb2/preflight".to_string()),
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let prompt = render_runtime_retry_prompt(&job, error);

        assert!(prompt.contains("HARNESS FEEDBACK"));
        assert!(prompt.contains("wrong shell action"));
        assert!(prompt.contains("creates the current RUN_DIR"));
        assert!(prompt.contains("do not inspect install trees"));
        assert!(prompt.contains("not work performed by the harness"));
        assert!(prompt.contains("HARNESS TERMINAL-BENCH PREFLIGHT RETRY"));
        assert!(prompt.contains("Only required durable files for this controller turn"));
        assert!(prompt.contains("/tmp/tb/controller.json"));
        assert!(prompt.contains("/tmp/tb/ticket-map.jsonl"));
        assert!(prompt.contains("/tmp/tb/preparation-tickets.jsonl"));
        assert!(prompt.contains("Do not satisfy this retry with substitute files"));
    }

    #[test]
    fn terminal_bench_preflight_retry_loop_stops_after_feedback_retry() {
        let error = "terminal-bench preflight violation: the first shell command did not create and verify the required current-run artifacts.";
        let first_attempt = QueuedPrompt {
            prompt:
                "Only required durable files for this controller turn:\n- /tmp/tb/controller.json"
                    .to_string(),
            goal: "Terminal-Bench controller preflight".to_string(),
            preview: "Terminal-Bench controller preflight".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tb2/preflight".to_string()),
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };
        assert!(!terminal_bench_preflight_retry_loop_should_stop(
            &first_attempt,
            error
        ));

        let retry_attempt = QueuedPrompt {
            prompt: format!(
                "{}\n\nHARNESS TERMINAL-BENCH PREFLIGHT RETRY\nOnly required durable files for this controller turn:\n- /tmp/tb/controller.json",
                first_attempt.prompt
            ),
            ..first_attempt
        };
        assert!(terminal_bench_preflight_retry_loop_should_stop(
            &retry_attempt,
            error
        ));
        assert!(terminal_bench_preflight_loop_stop_note(error).contains("Stopped automatic"));
    }

    #[test]
    fn leased_queue_runtime_retry_gets_harness_feedback_prompt() {
        let root = temp_root("leased-queue-runtime-retry-feedback");
        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Qwen smoke".to_string(),
                prompt: "Create and verify the smoke artifact.".to_string(),
                thread_key: "smoke/qwen".to_string(),
                workspace_root: Some("/tmp/qwen-smoke".to_string()),
                priority: "high".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("create queue task");
        channels::lease_queue_task(&root, &task.message_key, CHANNEL_ROUTER_LEASE_OWNER)
            .expect("lease queue task");
        let job = QueuedPrompt {
            prompt: task.prompt.clone(),
            goal: task.prompt.clone(),
            preview: task.title.clone(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec![task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some(task.thread_key.clone()),
            workspace_root: task.workspace_root.clone(),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let updated = apply_runtime_retry_feedback_to_leased_queue(
            &root,
            &job,
            "turn completed without assistant message",
        )
        .expect("inject feedback");
        assert_eq!(updated, 1);

        let reloaded = channels::load_queue_task(&root, &task.message_key)
            .expect("load queue task")
            .expect("queue task exists");
        assert!(reloaded.prompt.contains("HARNESS FEEDBACK"));
        assert!(reloaded
            .prompt
            .contains("without producing the required final assistant message"));
        assert!(reloaded
            .prompt
            .contains("Create and verify the smoke artifact."));
        assert_eq!(reloaded.workspace_root.as_deref(), Some("/tmp/qwen-smoke"));
        assert_eq!(route_status_for(&root, &task.message_key), "leased");
    }

    #[test]
    fn runtime_retry_prompt_strips_nested_harness_feedback() {
        let original =
            "Work only inside this workspace:\n/home/metricspace\n\nRun Terminal-Bench 2 cleanly.";
        let nested = format!(
            "HARNESS FEEDBACK\nProblem: x\n\nCURRENT TASK\n{original}\n\nRUNTIME FAILURE\ny\n\nREQUIRED ACTIONS\n- z"
        );
        let job = QueuedPrompt {
            prompt: nested.clone(),
            goal: nested,
            preview: "Terminal-Bench 2".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("tb2".to_string()),
            workspace_root: Some("/home/metricspace".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let prompt =
            render_runtime_retry_prompt(&job, "terminal-bench preflight violation: bad first call");

        assert!(prompt.contains("Run Terminal-Bench 2 cleanly."));
        assert_eq!(prompt.matches("RUNTIME FAILURE").count(), 1);
        assert!(!prompt.contains("CURRENT TASK\nHARNESS FEEDBACK"));
        assert!(prompt.starts_with("Work only inside this workspace:\n/home/metricspace"));
    }

    #[test]
    fn runtime_rate_limit_suppresses_standalone_retry_with_outbound_metadata() {
        let root = temp_root("runtime-rate-limit-retry");
        let outbound = channels::FounderOutboundAction {
            account_key: "email:cto1@metric-space.ai".to_string(),
            thread_key: "<founder-thread@example.com>".to_string(),
            subject: "Tag proposal".to_string(),
            to: vec!["j.kienzler@remcapital.de".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
        };
        let job = QueuedPrompt {
            prompt: "Send the reviewed founder email.".to_string(),
            goal: "Send the reviewed founder email to Julia".to_string(),
            preview: "reviewed-founder-send Julia".to_string(),
            source_label: "tui-outbound".to_string(),
            suggested_skill: Some("owner-communication".to_string()),
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("founder-outbound:julia-tag-proposal".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: Some(outbound.clone()),
            outbound_anchor: Some("tui-outbound:julia-tag-proposal".to_string()),
        };

        let created = maybe_enqueue_runtime_retry_continuation(
            &root,
            &job,
            "model call failed: status 429 Too Many Requests",
        )
        .expect("runtime retry should not fail");
        assert!(created.is_none());

        let tasks = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("queue task should be listed");
        assert!(
            tasks.is_empty(),
            "standalone API retry tasks must not be queued because they can loop after rate limits"
        );
    }

    #[test]
    fn non_work_tui_probe_is_ignored() {
        assert!(is_non_work_tui_probe("hello queue"));
        assert!(is_non_work_tui_probe("  health   check  "));
        assert!(!is_non_work_tui_probe(
            "CTO1, beantworte die Founder-Mail sauber"
        ));
    }

    #[test]
    fn boot_releases_stale_service_communication_leases() {
        let root = temp_root("boot-release-service-leases");
        upsert_test_inbound_message(
            &root,
            "queue:system::stale-founder-rework",
            "queue",
            "founder-thread",
            "system@local",
            "Founder communication rework",
            "Rework founder mail",
            json!({
                "origin_source_label": "email:founder",
                "parent_message_key": "email:cto1@metric-space.ai::INBOX::96",
            }),
        );
        let conn =
            channels::open_channel_db(&crate::paths::core_db(&root)).expect("open channel db");
        conn.execute(
            "UPDATE communication_routing_state SET route_status='leased', lease_owner=?2, leased_at='2026-04-28T20:00:00Z', acked_at=NULL WHERE message_key=?1",
            params!["queue:system::stale-founder-rework", CHANNEL_ROUTER_LEASE_OWNER],
        )
        .expect("seed stale lease");

        let repaired =
            release_stale_service_communication_leases(&root).expect("release stale leases");
        assert_eq!(repaired, 1);

        let row = conn
            .query_row(
                "SELECT route_status, lease_owner, leased_at FROM communication_routing_state WHERE message_key=?1",
                params!["queue:system::stale-founder-rework"],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .expect("route row");
        assert_eq!(row.0, "pending");
        assert_eq!(row.1, None);
        assert_eq!(row.2, None);
    }

    #[test]
    fn open_founder_inbound_blocks_strategy_reroute_for_queue_work() {
        let root = temp_root("ctox-open-founder-blocks-strategy-reroute");
        let mut runtime_settings = BTreeMap::new();
        runtime_settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &runtime_settings)
            .expect("failed to persist owner setting");
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', '<founder-thread@example.com>',
                'remote-founder-1', 'inbound', 'INBOX', 'Michael Welsch',
                'michael.welsch@metric-space.ai', '[]', '[]', '[]', 'Founder input',
                'Founder input', 'Please answer me before doing anything else.', '', '',
                'normal', 'received', 0, 0, '2026-04-24T18:55:00Z', '2026-04-24T18:55:00Z', '{}'
            )"#,
            rusqlite::params!["email:cto1@metric-space.ai::INBOX::91"],
        )
        .expect("failed to insert founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'pending', NULL, NULL, NULL, NULL, '2026-04-24T18:55:00Z')"#,
            rusqlite::params!["email:cto1@metric-space.ai::INBOX::91"],
        )
        .expect("failed to insert founder routing state");

        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Platform homepage work".to_string(),
                prompt: "Reset kunstmen.com so it behaves like a platform for hiring AI employees."
                    .to_string(),
                thread_key: "kunstmen-supervisor".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("follow-up-orchestrator".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        let job = QueuedPrompt {
            prompt: "Reset kunstmen.com so it behaves like a platform for hiring AI employees."
                .to_string(),
            goal: "Kunstmen platform homepage reset".to_string(),
            preview: "Kunstmen platform homepage reset".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-supervisor".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("strategy evaluation should succeed");
        assert!(!redirected);

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Platform homepage work");
    }

    #[test]
    fn founder_email_thread_is_not_rerouted_into_strategy_setup() {
        let root = temp_root("ctox-founder-email-no-strategy-reroute");
        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Founder inbound".to_string(),
                prompt: "[E-Mail eingegangen]\nSender: founder@example.com\nBetreff: Homepage\nPlease fix the public platform flow and answer me clearly."
                    .to_string(),
                thread_key: "<founder-thread@example.com>".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("frontend-skill".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed founder queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = lock_shared_state(&state);
            shared.busy = true;
            shared.current_goal_preview = Some("Founder inbound".to_string());
            shared.active_source_label = Some("email:founder".to_string());
            track_leased_keys_locked(
                &mut shared,
                std::slice::from_ref(&queue_task.message_key),
                &[],
            );
        }
        let job = QueuedPrompt {
            prompt: "[E-Mail eingegangen]\nSender: founder@example.com\nBetreff: Homepage\nPlease fix the public platform flow and answer me clearly."
                .to_string(),
            goal: "Reply to founder".to_string(),
            preview: "Founder mail about homepage".to_string(),
            source_label: "email:founder".to_string(),
            suggested_skill: Some("frontend-skill".to_string()),
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("<founder-thread@example.com>".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_owner_visible_work_to_strategy_setup(&root, &state, &job)
            .expect("strategy evaluation should succeed");
        assert!(!redirected);

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Founder inbound");
    }

    #[test]
    fn founder_email_thread_is_not_rerouted_into_platform_passes() {
        let root = temp_root("ctox-founder-email-no-platform-reroute");
        let queue_task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Founder inbound".to_string(),
                prompt: "[E-Mail eingegangen]\nSender: founder@example.com\nBetreff: Homepage\nThis platform is too noisy; simplify the page and make interview flow obvious."
                    .to_string(),
                thread_key: "<founder-thread@example.com>".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("frontend-skill".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed founder queue task");
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = lock_shared_state(&state);
            shared.busy = true;
            shared.current_goal_preview = Some("Founder inbound".to_string());
            shared.active_source_label = Some("email:founder".to_string());
            track_leased_keys_locked(
                &mut shared,
                std::slice::from_ref(&queue_task.message_key),
                &[],
            );
        }
        let job = QueuedPrompt {
            prompt: "[E-Mail eingegangen]\nSender: founder@example.com\nBetreff: Homepage\nThis platform is too noisy; simplify the page and make interview flow obvious."
                .to_string(),
            goal: "Reply to founder".to_string(),
            preview: "Founder mail about homepage".to_string(),
            source_label: "email:founder".to_string(),
            suggested_skill: Some("frontend-skill".to_string()),
            leased_message_keys: vec![queue_task.message_key.clone()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("<founder-thread@example.com>".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let redirected = maybe_redirect_platform_work_to_expertise_passes(&root, &state, &job)
            .expect("platform evaluation should succeed");
        assert!(!redirected);

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert!(items.is_empty());
    }

    #[test]
    fn founder_review_rejection_enqueues_real_communication_rework() {
        let root = temp_root("ctox-founder-communication-rework");
        let job = QueuedPrompt {
            prompt: "[E-Mail eingegangen]\nSender: michael.welsch@metric-space.ai\nBetreff: Jami zugang schicken.\nSchick mir bitte den Jami QR code Zugang fuer den Chat mit dir."
                .to_string(),
            goal: "Reply to founder".to_string(),
            preview: "Founder asks for Jami QR code".to_string(),
            source_label: "email:owner".to_string(),
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            leased_message_keys: vec!["email:cto1@metric-space.ai::INBOX::82".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("<founder-thread@example.com>".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };
        let outcome = review::ReviewOutcome {
            required: true,
            verdict: review::ReviewVerdict::Fail,
            mission_state: "HEALTHY".to_string(),
            summary: "Owner requested a QR code and the draft does not include it.".to_string(),
            report: String::new(),
            score: 21,
            reasons: vec!["missing_deliverable".to_string()],
            failed_gates: vec!["missing_deliverable".to_string()],
            semantic_findings: vec!["QR code is required before any reply can be sent.".to_string()],
            categorized_findings: Vec::new(),
            open_items: vec!["Generate or retrieve the Jami QR code.".to_string()],
            evidence: vec!["owner mail explicitly asks for QR code".to_string()],
            handoff: None,
            disposition: review::ReviewDisposition::Send,
        };

        let title = enqueue_founder_communication_rework(
            &root,
            &job,
            "email:cto1@metric-space.ai::INBOX::82",
            &outcome,
        )
        .expect("founder communication rework should enqueue");
        assert!(title.starts_with("Founder communication rework:"));

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, title);
        assert!(tasks[0]
            .prompt
            .contains("Beende diesen Arbeitsschritt mit genau der E-Mail"));
        assert!(tasks[0]
            .prompt
            .contains("Generate or retrieve the Jami QR code."));
        assert!(tasks[0].ticket_self_work_id.is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stalled_founder_email_requeues_blocked_rework() {
        let root = temp_root("ctox-stalled-founder-repair");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &settings)
            .expect("failed to persist owner setting");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::99";
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', '<olaf-thread@example.com>',
                'remote-founder-99', 'inbound', 'INBOX', 'Olaf Schaefers',
                'michael.welsch@metric-space.ai', '[]', '[]', '[]',
                'Aw: Kunstmen Wettbewerbsdashboard: erster Entwurf',
                'Founder asks whether founder-only belongs on the frontpage',
                'Ist founder-only noch richtig, wenn das auf der Frontpage steht?',
                '', '', 'normal', 'received', 0, 1,
                '2026-04-28T12:23:00Z', '2026-04-28T12:23:00Z', '{}'
            )"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'failed', NULL, NULL, NULL, NULL, '2026-04-28T15:46:00Z')"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert stalled founder route");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: FOUNDER_COMMUNICATION_REWORK_KIND.to_string(),
                title: "Founder communication rework: founder-only".to_string(),
                body_text: "Answer Olaf after checking the screenshot.".to_string(),
                state: "blocked".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "email-review:founder:test",
                    "priority": "urgent",
                    "skill": "follow-up-orchestrator",
                    "parent_message_key": inbound_key,
                    "inbound_message_key": inbound_key,
                    "dedupe_key": format!("founder-communication-rework:{inbound_key}"),
                }),
            },
            false,
        )
        .expect("failed to seed blocked rework");
        tickets::assign_ticket_self_work_item(
            &root,
            &item.work_id,
            "self",
            "test",
            Some("founder repair test"),
        )
        .expect("failed to assign blocked rework");

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired = repair_stalled_founder_communications(&root, &state, &settings)
            .expect("stalled founder repair should succeed");
        assert_eq!(repaired, 1);

        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].ticket_self_work_id.as_deref(),
            Some(item.work_id.as_str())
        );
        assert_eq!(tasks[0].parent_message_key.as_deref(), Some(inbound_key));
        let origin_source: String = conn
            .query_row(
                "SELECT json_extract(metadata_json, '$.origin_source_label') FROM communication_messages WHERE message_key = ?1",
                rusqlite::params![tasks[0].message_key],
                |row| row.get(0),
            )
            .expect("failed to load queue origin source label");
        assert_eq!(origin_source, "email:founder");
        let route_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                rusqlite::params![inbound_key],
                |row| row.get(0),
            )
            .expect("failed to reload route status");
        assert_eq!(route_status, "review_rework");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn repeated_founder_rework_review_rejections_block_the_loop() {
        let root = temp_root("ctox-founder-rework-loop-block");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::loop";
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: FOUNDER_COMMUNICATION_REWORK_KIND.to_string(),
                title: "Founder communication rework: CRM update".to_string(),
                body_text: "Answer the founder after doing the CRM work.".to_string(),
                state: "queued".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "email-review:founder:loop",
                    "priority": "urgent",
                    "skill": "follow-up-orchestrator",
                    "parent_message_key": inbound_key,
                    "inbound_message_key": inbound_key,
                    "dedupe_key": format!("founder-communication-rework:{inbound_key}"),
                }),
            },
            false,
        )
        .expect("failed to seed founder rework");

        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        for attempt in 0..FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD {
            let task = channels::create_queue_task(
                &root,
                channels::QueueTaskCreateRequest {
                    title: format!("Founder communication rework: CRM update {attempt}"),
                    prompt: "Review rejected the founder reply; do real rework.".to_string(),
                    thread_key: "email-review:founder:loop".to_string(),
                    workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                    priority: "urgent".to_string(),
                    suggested_skill: Some("follow-up-orchestrator".to_string()),
                    parent_message_key: Some(inbound_key.to_string()),
                    extra_metadata: Some(serde_json::json!({
                        "ticket_self_work_id": item.work_id.clone(),
                        "ticket_self_work_kind": FOUNDER_COMMUNICATION_REWORK_KIND,
                        "parent_message_key": inbound_key,
                        "inbound_message_key": inbound_key,
                    })),
                },
            )
            .expect("failed to create queued attempt");
            conn.execute(
                "UPDATE communication_routing_state SET route_status = 'review_rework' WHERE message_key = ?1",
                params![task.message_key],
            )
            .expect("failed to mark attempt as review rework");
        }

        let queued = requeue_review_rejected_self_work(
            &root,
            &item.work_id,
            "The reply still lacks CRM evidence and only restates intent.",
        )
        .expect("review requeue should be handled");
        assert!(queued.is_none());

        let reloaded = tickets::load_ticket_self_work_item(&root, &item.work_id)
            .expect("failed to reload self-work")
            .expect("missing self-work");
        assert_eq!(reloaded.state, "blocked");
        let open_tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list open queue tasks");
        assert!(open_tasks.is_empty());
        let blocked_tasks = channels::list_queue_tasks(&root, &["blocked".to_string()], 10)
            .expect("failed to list blocked queue tasks");
        assert_eq!(blocked_tasks.len(), FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD);
        assert!(blocked_tasks.iter().all(|task| task
            .status_note
            .as_deref()
            .unwrap_or("")
            .contains("neue belastbare Grundlage")));

        let note_count_after_first_block: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ticket_self_work_notes WHERE work_id = ?1",
                params![item.work_id],
                |row| row.get(0),
            )
            .expect("failed to count notes after first block");
        assert_eq!(note_count_after_first_block, 1);

        let queued = requeue_review_rejected_self_work(
            &root,
            &item.work_id,
            "The reply is still only a rewrite without new CRM evidence.",
        )
        .expect("repeated loop block should be idempotent");
        assert!(queued.is_none());
        let note_count_after_second_block: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ticket_self_work_notes WHERE work_id = ?1",
                params![item.work_id],
                |row| row.get(0),
            )
            .expect("failed to count notes after second block");
        assert_eq!(note_count_after_second_block, note_count_after_first_block);
        let _ = std::fs::remove_dir_all(root);
    }

    /// Bug #4 helper: routed inbound that lives on a deterministic
    /// raw thread-key. Tests in this module use the raw thread-key;
    /// the production code derives the isolated key via
    /// `isolated_founder_email_thread_key`.
    fn routed_founder_inbound(
        message_key: &str,
        thread_key: &str,
    ) -> channels::RoutedInboundMessage {
        channels::RoutedInboundMessage {
            message_key: message_key.to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@metric-space.ai".to_string(),
            thread_key: thread_key.to_string(),
            sender_display: "Jill Cakmak".to_string(),
            sender_address: "j.cakmak@remcapital.de".to_string(),
            subject: "Re: Förderanträge".to_string(),
            preview: "preview".to_string(),
            body_text: "body".to_string(),
            external_created_at: "2026-04-29T19:42:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        }
    }

    /// Bug #4 baseline: with no prior rework on the thread,
    /// `ensure_founder_communication_rework_runnable` spawns a fresh
    /// rework self-work-item for the inbound message.
    #[test]
    fn founder_rework_baseline_spawns_when_no_prior_work_on_thread() {
        let root = temp_root("ctox-founder-rework-bug4-baseline");
        let inbound = routed_founder_inbound(
            "email:cto1@metric-space.ai::INBOX::bug4-baseline",
            "raw-thread-bug4-baseline",
        );

        let changed = ensure_founder_communication_rework_runnable(
            &root,
            &inbound,
            "Founder mail blieb ohne geprüften Versand stehen.",
        )
        .expect("rework runnable check should not error");
        assert!(changed, "first rework on the thread must spawn");

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 64)
            .expect("failed to list self-work");
        let founder_items: Vec<_> = items
            .iter()
            .filter(|item| item.kind == FOUNDER_COMMUNICATION_REWORK_KIND)
            .collect();
        assert_eq!(
            founder_items.len(),
            1,
            "exactly one founder rework self-work-item should be created"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Bug #4 existing dedupe: a repeated call with the same
    /// `inbound_message_key` and an `open` prior rework re-queues
    /// the existing item rather than spawning a duplicate.
    #[test]
    fn founder_rework_repeat_with_same_inbound_key_does_not_duplicate() {
        let root = temp_root("ctox-founder-rework-bug4-same-key");
        let inbound = routed_founder_inbound(
            "email:cto1@metric-space.ai::INBOX::bug4-same-key",
            "raw-thread-bug4-same-key",
        );

        ensure_founder_communication_rework_runnable(
            &root,
            &inbound,
            "Founder mail blieb ohne geprüften Versand stehen.",
        )
        .expect("first call must succeed");

        ensure_founder_communication_rework_runnable(
            &root,
            &inbound,
            "Founder mail blieb ohne geprüften Versand stehen.",
        )
        .expect("second call must succeed");

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 64)
            .expect("failed to list self-work");
        let founder_items: Vec<_> = items
            .iter()
            .filter(|item| item.kind == FOUNDER_COMMUNICATION_REWORK_KIND)
            .collect();
        assert_eq!(
            founder_items.len(),
            1,
            "existing inbound-message-key dedupe must prevent a second spawn"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Bug #4 fix: a NEW founder mail arriving on the same thread
    /// while a prior rework is `blocked` by the review-loop
    /// circuit-breaker MUST NOT spawn a fresh rework on a new
    /// `work_id`. Trigger is purely structural (state == blocked AND
    /// thread_key match), no string-heuristics.
    #[test]
    fn founder_rework_new_inbound_on_same_thread_blocked_by_circuit() {
        let root = temp_root("ctox-founder-rework-bug4-thread-block");
        let raw_thread = "raw-thread-bug4-thread-block";
        let isolated_thread = isolated_founder_email_thread_key(raw_thread, "founder");
        let prior_inbound_key = "email:cto1@metric-space.ai::INBOX::bug4-thread-block-prior";

        // Seed a prior rework self-work-item already in `blocked`
        // state (mirrors what `requeue_review_rejected_self_work` does
        // after FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD review loops).
        let seeded = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: FOUNDER_COMMUNICATION_REWORK_KIND.to_string(),
                title: "Founder communication rework: prior".to_string(),
                body_text: "Existing prior rework.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": isolated_thread,
                    "priority": "urgent",
                    "skill": "follow-up-orchestrator",
                    "parent_message_key": prior_inbound_key,
                    "inbound_message_key": prior_inbound_key,
                    "dedupe_key": format!("founder-communication-rework:{prior_inbound_key}"),
                }),
            },
            false,
        )
        .expect("failed to seed prior rework");
        tickets::transition_ticket_self_work_item(
            &root,
            &seeded.work_id,
            "blocked",
            "ctox-test",
            Some("circuit-breaker reached threshold"),
            "internal",
        )
        .expect("failed to transition prior rework to blocked");

        // A NEW inbound mail on the SAME thread arrives -- different
        // `message_key`, same `thread_key`.
        let new_inbound = routed_founder_inbound(
            "email:cto1@metric-space.ai::INBOX::bug4-thread-block-new",
            raw_thread,
        );

        let changed = ensure_founder_communication_rework_runnable(
            &root,
            &new_inbound,
            "Neue Founder-Mail auf demselben Thread während prior=blocked.",
        )
        .expect("rework runnable check should not error");
        assert!(
            !changed,
            "thread-scoped circuit-breaker must veto fresh rework spawn"
        );

        // Exactly one founder rework self-work-item should still exist
        // -- the original blocked one. No new work_id was minted.
        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 64)
            .expect("failed to list self-work");
        let founder_items: Vec<_> = items
            .iter()
            .filter(|item| item.kind == FOUNDER_COMMUNICATION_REWORK_KIND)
            .collect();
        assert_eq!(
            founder_items.len(),
            1,
            "no new founder rework self-work-item must be spawned (Bug #4)"
        );
        assert_eq!(founder_items[0].work_id, seeded.work_id);
        assert_eq!(founder_items[0].state, "blocked");

        // No new founder rework queue task should have been enqueued.
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 16)
                .expect("failed to list queue tasks");
        assert!(
            tasks
                .iter()
                .all(|task| !task.title.starts_with("Founder communication rework:")),
            "no new founder rework should have been queued"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Bug #4 over-block guard: a NEW founder mail arriving on a
    /// DIFFERENT thread (even when a prior rework on another thread
    /// is `blocked`) MUST still spawn a fresh rework. The
    /// circuit-breaker is thread-scoped, not global.
    #[test]
    fn founder_rework_new_inbound_on_different_thread_still_spawns() {
        let root = temp_root("ctox-founder-rework-bug4-different-thread");
        let blocked_raw_thread = "raw-thread-bug4-blocked";
        let blocked_isolated_thread =
            isolated_founder_email_thread_key(blocked_raw_thread, "founder");
        let blocked_inbound_key = "email:cto1@metric-space.ai::INBOX::bug4-different-blocked";

        let seeded = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: FOUNDER_COMMUNICATION_REWORK_KIND.to_string(),
                title: "Founder communication rework: blocked thread".to_string(),
                body_text: "Existing prior rework on the blocked thread.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": blocked_isolated_thread,
                    "priority": "urgent",
                    "skill": "follow-up-orchestrator",
                    "parent_message_key": blocked_inbound_key,
                    "inbound_message_key": blocked_inbound_key,
                    "dedupe_key": format!("founder-communication-rework:{blocked_inbound_key}"),
                }),
            },
            false,
        )
        .expect("failed to seed prior rework");
        tickets::transition_ticket_self_work_item(
            &root,
            &seeded.work_id,
            "blocked",
            "ctox-test",
            Some("circuit-breaker reached threshold"),
            "internal",
        )
        .expect("failed to transition prior rework to blocked");

        // A NEW inbound mail on a DIFFERENT thread arrives.
        let other_inbound = routed_founder_inbound(
            "email:cto1@metric-space.ai::INBOX::bug4-different-other",
            "raw-thread-bug4-other",
        );

        let changed = ensure_founder_communication_rework_runnable(
            &root,
            &other_inbound,
            "Neue Founder-Mail auf anderem Thread.",
        )
        .expect("rework runnable check should not error");
        assert!(
            changed,
            "different-thread inbound must still spawn its own rework"
        );

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 64)
            .expect("failed to list self-work");
        let founder_items: Vec<_> = items
            .iter()
            .filter(|item| item.kind == FOUNDER_COMMUNICATION_REWORK_KIND)
            .collect();
        assert_eq!(
            founder_items.len(),
            2,
            "different-thread inbound must produce a second self-work-item"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn channel_router_does_not_repair_founder_mail_during_active_agent_loop() {
        let root = temp_root("ctox-founder-router-active-loop");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &settings)
            .expect("failed to persist owner setting");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::active-loop";
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', '<founder-active-loop@example.com>',
                'remote-founder-active-loop', 'inbound', 'INBOX', 'Michael Welsch',
                'michael.welsch@metric-space.ai', '[]', '[]', '[]',
                'Aw: Kunstmen CRM',
                'Founder asks for CRM update while work is active.',
                'Bitte CRM erst sauber fertigstellen und dann antworten.',
                '', '', 'normal', 'received', 0, 0,
                '2026-04-29T20:00:00Z', '2026-04-29T20:00:00Z', '{}'
            )"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'failed', NULL, NULL, NULL, NULL, '2026-04-29T20:01:00Z')"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert failed founder route");

        let state = Arc::new(Mutex::new(SharedState {
            busy: true,
            ..SharedState::default()
        }));
        route_external_messages(&root, &state).expect("busy channel router pass should not fail");

        let route_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                rusqlite::params![inbound_key],
                |row| row.get(0),
            )
            .expect("failed to reload route status");
        assert_eq!(route_status, "failed");
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert!(
            tasks.is_empty(),
            "router must not create founder rework while an agent loop is active"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stalled_founder_email_superseded_by_later_reviewed_thread_send_is_cancelled() {
        let root = temp_root("ctox-stalled-founder-superseded");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &settings)
            .expect("failed to persist owner setting");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::94";
        let thread_key = "<founder-thread@example.com>";
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', ?2,
                'remote-founder-94', 'inbound', 'INBOX', 'Michael Welsch',
                'michael.welsch@metric-space.ai', '[]', '[]', '[]',
                'Aw: Re: Visuelle Homepage',
                'Earlier founder mail now covered by a later reviewed reply.',
                'Earlier founder mail now covered by a later reviewed reply.',
                '', '', 'normal', 'received', 0, 0,
                '2026-04-27T09:01:02Z', '2026-04-27T09:01:02Z', '{}'
            )"#,
            rusqlite::params![inbound_key, thread_key],
        )
        .expect("failed to insert founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'failed', NULL, NULL, '2026-04-27T11:55:27Z', NULL, '2026-04-27T11:55:27Z')"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert failed route");
        conn.execute(
            r#"INSERT INTO communication_founder_reply_reviews (
                approval_key, inbound_message_key, action_digest, action_json, body_sha256,
                reviewer, review_summary, approved_at, sent_at, send_result_json
            ) VALUES (
                'review-later-thread-send', 'email:cto1@metric-space.ai::INBOX::95',
                'digest', ?1, 'body-sha', 'external-review',
                'later reviewed founder reply in same thread',
                '2026-04-27T15:39:18Z', '2026-04-27T15:39:18Z', '{}'
            )"#,
            rusqlite::params![serde_json::json!({
                "thread_key": thread_key,
                "to": ["michael.welsch@metric-space.ai"],
                "cc": [],
                "subject": "Re: Aw: Re: Visuelle Homepage",
                "attachments": [],
            })
            .to_string()],
        )
        .expect("failed to seed later reviewed send");

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired = repair_stalled_founder_communications(&root, &state, &settings)
            .expect("stalled founder repair should succeed");
        assert!(repaired >= 1);
        let route_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                rusqlite::params![inbound_key],
                |row| row.get(0),
            )
            .expect("failed to reload route status");
        assert_eq!(route_status, "cancelled");
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert!(tasks.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn synthetic_no_send_review_does_not_count_as_founder_send_proof() {
        let root = temp_root("ctox-synthetic-no-send-not-send-proof");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "marco@example.com".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &settings)
            .expect("failed to persist owner setting");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::100";
        let thread_key = "<dashboard-thread@example.com>";
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', ?2,
                'remote-founder-100', 'inbound', 'INBOX', 'Marco',
                'marco@example.com', '[]', '[]', '[]',
                'AW: Kunstmen Wettbewerbsdashboard',
                'Please add market, funding, and investor research.',
                'Please add market, funding, and investor research.',
                '', '', 'normal', 'received', 0, 0,
                '2026-04-29T06:31:57Z', '2026-04-29T06:31:57Z', '{}'
            )"#,
            rusqlite::params![inbound_key, thread_key],
        )
        .expect("failed to insert founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'failed', NULL, NULL, NULL, NULL, '2026-04-29T09:14:27Z')"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert failed route");
        conn.execute(
            r#"INSERT INTO communication_founder_reply_reviews (
                approval_key, inbound_message_key, action_digest, action_json, body_sha256,
                reviewer, review_summary, approved_at, sent_at, send_result_json
            ) VALUES (
                'synthetic-no-send', ?1, 'digest-no-send', ?2, 'body-sha',
                'codex-no-send', 'NO-SEND: wait for a different CRM thread',
                '2026-04-29T10:25:21Z', '2026-04-29T10:25:21Z',
                '{"channel":"email","ok":true,"status":"no-send-recorded","synthetic":true}'
            )"#,
            rusqlite::params![
                inbound_key,
                serde_json::json!({
                    "thread_key": thread_key,
                    "to": ["marco@example.com"],
                    "cc": [],
                    "subject": "Re: AW: Kunstmen Wettbewerbsdashboard",
                    "attachments": [],
                })
                .to_string()
            ],
        )
        .expect("failed to insert synthetic no-send proof");

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired = repair_stalled_founder_communications(&root, &state, &settings)
            .expect("stalled founder repair should succeed");
        assert!(repaired >= 1);
        let route_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                rusqlite::params![inbound_key],
                |row| row.get(0),
            )
            .expect("failed to reload route status");
        assert_eq!(route_status, "review_rework");
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].parent_message_key.as_deref(), Some(inbound_key));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn blocked_founder_inbound_is_not_auto_restored() {
        let root = temp_root("ctox-blocked-founder-not-restored");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &settings)
            .expect("failed to persist owner setting");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::100";
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', '<dashboard-thread@example.com>',
                'remote-founder-100', 'inbound', 'INBOX', 'Michael Welsch',
                'michael.welsch@metric-space.ai', '[]', '[]', '[]',
                'AW: Kunstmen Wettbewerbsdashboard',
                'This founder mail is intentionally paused.',
                'This founder mail is intentionally paused.',
                '', '', 'normal', 'received', 0, 0,
                '2026-04-29T06:31:57Z', '2026-04-29T06:31:57Z', '{}'
            )"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'blocked', NULL, NULL, NULL, 'operator paused behind newer founder mail', '2026-04-29T09:14:27Z')"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert blocked route");

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired = repair_stalled_founder_communications(&root, &state, &settings)
            .expect("stalled founder repair should succeed");
        assert_eq!(repaired, 0);
        let route_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                rusqlite::params![inbound_key],
                |row| row.get(0),
            )
            .expect("failed to reload route status");
        assert_eq!(route_status, "blocked");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stalled_founder_email_not_superseded_by_later_cross_thread_sender_send() {
        let root = temp_root("ctox-stalled-founder-cross-thread-sender-not-superseded");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &settings)
            .expect("failed to persist owner setting");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::96";
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', '<old-thread@example.com>',
                'remote-founder-96', 'inbound', 'INBOX', 'Michael Welsch',
                'michael.welsch@metric-space.ai', '[]', '[]', '[]',
                'Re: Visuelle Homepage',
                'Older founder mail covered by a later reviewed cross-thread send.',
                'Older founder mail covered by a later reviewed cross-thread send.',
                '', '', 'normal', 'received', 0, 0,
                '2026-04-27T15:48:00Z', '2026-04-27T15:48:00Z', '{}'
            )"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'review_rework', NULL, NULL, NULL, NULL, '2026-04-29T03:35:18Z')"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert review route");
        let task = channels::create_queue_task_with_metadata(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Founder communication rework: Re: Visuelle Homepage".to_string(),
                prompt: "Reply to stale founder mail.".to_string(),
                thread_key: "email-review:founder:old-thread".to_string(),
                workspace_root: None,
                priority: "urgent".to_string(),
                suggested_skill: Some("follow-up-orchestrator".to_string()),
                parent_message_key: Some(inbound_key.to_string()),
                extra_metadata: Some(serde_json::json!({
                    "inbound_message_key": inbound_key,
                    "ticket_self_work_kind": FOUNDER_COMMUNICATION_REWORK_KIND,
                })),
            },
        )
        .expect("failed to create stale queue task");
        channels::ack_leased_messages(&root, std::slice::from_ref(&task.message_key), "leased")
            .expect("failed to lease stale queue task");
        conn.execute(
            r#"INSERT INTO communication_founder_reply_reviews (
                approval_key, inbound_message_key, action_digest, action_json, body_sha256,
                reviewer, review_summary, approved_at, sent_at, send_result_json
            ) VALUES (
                'review-later-cross-thread-send', 'email:cto1@metric-space.ai::INBOX::99',
                'digest-cross-thread', ?1, 'body-sha-cross-thread', 'external-review',
                'later reviewed founder reply copied Michael on a different founder thread',
                '2026-04-27T23:21:23Z', '2026-04-27T23:21:23Z', '{"ok":true}'
            )"#,
            rusqlite::params![serde_json::json!({
                "thread_key": "<current-thread@example.com>",
                "to": ["o.schaefers@gmx.net"],
                "cc": ["michael.welsch@metric-space.ai"],
                "subject": "Re: Aw: Re: Visuelle Homepage",
                "attachments": [],
            })
            .to_string()],
        )
        .expect("failed to seed later reviewed send");

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired = repair_stalled_founder_communications(&root, &state, &settings)
            .expect("stalled founder repair should succeed");
        assert_eq!(
            repaired, 0,
            "a different thread to the same founder must not close this founder mail"
        );
        let route_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                rusqlite::params![inbound_key],
                |row| row.get(0),
            )
            .expect("failed to reload route status");
        assert_eq!(route_status, "review_rework");
        let task_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                rusqlite::params![task.message_key],
                |row| row.get(0),
            )
            .expect("failed to reload queue route status");
        assert_eq!(task_status, "leased");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reviewed_founder_reply_closes_stale_rework_item() {
        let root = temp_root("ctox-stale-founder-rework-close");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &settings)
            .expect("failed to persist owner setting");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::100";
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', '<founder-thread@example.com>',
                'remote-founder-100', 'inbound', 'INBOX', 'Michael Welsch',
                'michael.welsch@metric-space.ai', '[]', '[]', '[]',
                'Re: Affiliate Programm',
                'Founder asks for affiliate correction',
                'Bitte nicht auf der Landing Page bewerben.',
                '', '', 'normal', 'received', 0, 0,
                '2026-04-28T12:23:00Z', '2026-04-28T12:23:00Z', '{}'
            )"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'handled', NULL, NULL, '2026-04-28T12:40:00Z', NULL, '2026-04-28T12:40:00Z')"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert handled route");
        conn.execute(
            r#"INSERT INTO communication_founder_reply_reviews (
                approval_key, inbound_message_key, action_digest, action_json,
                body_sha256, reviewer, review_summary, approved_at, sent_at, send_result_json
            ) VALUES (
                'approval-founder-100', ?1, 'digest-founder-100', '{}',
                'body-founder-100', 'external-review', 'PASS: reviewed and sent',
                '2026-04-28T12:39:00Z', '2026-04-28T12:39:30Z', '{"ok":true}'
            )"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert review send proof");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: FOUNDER_COMMUNICATION_REWORK_KIND.to_string(),
                title: "Founder communication rework: affiliate".to_string(),
                body_text: "Answer founder after real rework.".to_string(),
                state: "queued".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "email-review:founder:stale-close",
                    "priority": "urgent",
                    "skill": "follow-up-orchestrator",
                    "parent_message_key": inbound_key,
                    "inbound_message_key": inbound_key,
                }),
            },
            false,
        )
        .expect("failed to seed queued rework");

        let state = Arc::new(Mutex::new(SharedState::default()));
        let repaired = repair_stalled_founder_communications(&root, &state, &settings)
            .expect("stale founder cleanup should succeed");
        assert_eq!(repaired, 1);
        let item = tickets::load_ticket_self_work_item(&root, &item.work_id)
            .expect("failed to reload self work")
            .expect("self work should exist");
        assert_eq!(item.state, "closed");
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert!(tasks.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn self_work_prompt_includes_latest_review_notes() {
        let root = temp_root("ctox-self-work-review-notes");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: FOUNDER_COMMUNICATION_REWORK_KIND.to_string(),
                title: "Founder communication rework: affiliate".to_string(),
                body_text: "Answer Michael in the existing founder thread.".to_string(),
                state: "queued".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "email-review:founder:notes",
                    "priority": "urgent",
                    "parent_message_key": "email:cto1@metric-space.ai::INBOX::96",
                }),
            },
            false,
        )
        .expect("failed to seed self work");
        tickets::append_ticket_self_work_note(
            &root,
            &item.work_id,
            "External review rejected the last slice: ask for a concrete affiliate decision and do not claim implementation is done.",
            "ctox-review",
            "internal",
        )
        .expect("failed to append review note");

        let prompt = render_ticket_self_work_prompt(&root, &item);
        assert!(prompt.contains("Aktuelle Rework- und Review-Hinweise"));
        assert!(prompt.contains("ask for a concrete affiliate decision"));
        assert!(prompt.contains("do not claim implementation is done"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn founder_rework_execution_prompt_strips_self_work_wrapper() {
        let root = temp_root("ctox-founder-rework-clean-prompt");
        let message = channels::RoutedInboundMessage {
            message_key: "queue:system::abc".to_string(),
            channel: "queue".to_string(),
            account_key: "system".to_string(),
            thread_key: "thread".to_string(),
            sender_display: "system".to_string(),
            sender_address: "system".to_string(),
            subject: "Founder communication rework: Affiliate reply".to_string(),
            preview: String::new(),
            body_text: String::new(),
            external_created_at: String::new(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };
        let raw = "Bearbeite das veroeffentlichte CTOX-Self-Work fuer local.\n\
Titel: Founder communication rework: Affiliate reply\n\
Art: founder-communication-rework\n\
Work-ID: self-work:local:123\n\n\
Review summary: Die Antwort geht nicht auf Olafs Korrektur ein.\n\
Was jetzt zu tun ist:\n\
- Entferne bits & birds und oeffentliche Prozentversprechen.";

        let prompt = render_founder_communication_rework_execution_prompt(
            &root,
            &message,
            "email:cto1@metric-space.ai::INBOX::97",
            raw,
        );
        assert!(!prompt.contains("CTOX-Self-Work"));
        assert!(!prompt.contains("Work-ID:"));
        assert!(!prompt.contains("Art:"));
        assert!(prompt.contains("Kurzfassung: Die Antwort geht nicht auf Olafs Korrektur ein."));
        assert!(prompt.contains("ausschliesslich den sendefertigen E-Mail-Text"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn detects_founder_deadline_commitments_in_mail_body() {
        let commitments = detect_founder_mail_commitments(
            "Hi Michael. Today, 24.04.2026, I send you an update by 20:00 UTC. Tomorrow, 25.04.2026, I will deliver the full redesign by 12:00 UTC.",
        );
        assert_eq!(commitments.len(), 2);
        assert!(commitments[0].contains("20:00 UTC"));
        assert!(commitments[1].contains("12:00 UTC"));
    }

    #[test]
    fn founder_commitment_guard_fails_without_backing() {
        let outcome = founder_commitment_guard_outcome(
            &[
                "Today, 24.04.2026, I send you an update by 20:00 UTC".to_string(),
                "Tomorrow, 25.04.2026, I will deliver the redesign by 12:00 UTC".to_string(),
            ],
            &[],
        )
        .expect("unbacked commitments should fail");
        assert_eq!(outcome.verdict, review::ReviewVerdict::Fail);
        assert_eq!(
            outcome.failed_gates,
            vec!["unbacked_commitment".to_string()]
        );
        assert!(outcome.summary.contains("2 future commitment(s)"));
    }

    #[test]
    fn founder_commitment_guard_allows_backed_deadlines() {
        let outcome = founder_commitment_guard_outcome(
            &[
                "Today, 24.04.2026, I send you an update by 20:00 UTC".to_string(),
                "Tomorrow, 25.04.2026, I will deliver the redesign by 12:00 UTC".to_string(),
            ],
            &[
                "kunstmen founder update 20utc @ 2026-04-24T20:00:00+00:00".to_string(),
                "kunstmen founder deliverable 12utc @ 2026-04-25T12:00:00+00:00".to_string(),
            ],
        );
        assert!(outcome.is_none());
    }

    #[test]
    fn successful_expertise_pass_queues_next_required_pass() {
        let root = temp_root("ctox-platform-pass-advance");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: PLATFORM_EXPERTISE_KIND.to_string(),
                title: "Kunstmen platform IA pass".to_string(),
                body_text: "Do the platform IA pass.".to_string(),
                state: "closed".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "kunstmen-supervisor",
                    "workspace_root": "/home/ubuntu/workspace/kunstmen",
                    "priority": "urgent",
                    "skill": "plan-orchestrator",
                    "pass_kind": "platform-ia",
                    "resume_prompt": "Build the platform front door.",
                    "resume_goal": "Kunstmen platform homepage reset",
                    "resume_preview": "Kunstmen platform homepage reset",
                    "resume_skill": "frontend-skill",
                    "dedupe_key": "platform-pass:kunstmen-supervisor:platform-ia",
                }),
            },
            false,
        )
        .expect("failed to create completed expertise pass");

        let job = QueuedPrompt {
            prompt: "Do the platform IA pass.".to_string(),
            goal: "Kunstmen platform homepage reset".to_string(),
            preview: "Kunstmen platform IA pass".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("plan-orchestrator".to_string()),
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-supervisor".to_string()),
            workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
            ticket_self_work_id: Some(item.work_id.clone()),
            outbound_email: None,
            outbound_anchor: None,
        };

        let note = maybe_continue_platform_expertise_pipeline_after_success(&root, &job)
            .expect("platform pass continuation should succeed");
        assert!(note
            .as_deref()
            .unwrap_or_default()
            .contains("messaging-wording"));

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        assert!(items.iter().any(|entry| {
            entry.kind == PLATFORM_EXPERTISE_KIND
                && platform_expertise_pass_kind(entry).as_deref() == Some("messaging-wording")
        }));
    }

    #[test]
    fn platform_passes_do_not_embed_full_prior_prompts() {
        let root = temp_root("ctox-platform-pass-compacts-resume");
        let long_prompt = "This prior prompt must not be copied wholesale. ".repeat(20_000);
        queue_platform_expertise_pass(
            &root,
            "kunstmen-supervisor",
            Some("/home/ubuntu/workspace/kunstmen"),
            PLATFORM_EXPERTISE_PASSES[0],
            &long_prompt,
            &long_prompt,
            &long_prompt,
            Some("frontend-skill"),
        )
        .expect("failed to queue compacted platform pass");

        let items = tickets::list_ticket_self_work_items(&root, Some("local"), None, 10)
            .expect("failed to list self-work");
        let item = items
            .iter()
            .find(|entry| entry.kind == PLATFORM_EXPERTISE_KIND)
            .expect("missing platform expertise self-work");
        assert!(
            item.body_text.chars().count() < 6_000,
            "body_text should be compact, got {} chars",
            item.body_text.chars().count()
        );
        let resume_prompt = platform_expertise_resume_prompt(item).unwrap_or_default();
        assert!(
            resume_prompt.chars().count() <= DEFERRED_METADATA_MAX_CHARS + 1,
            "resume metadata should be compact, got {} chars",
            resume_prompt.chars().count()
        );
    }

    #[test]
    fn review_requeue_closes_superseded_platform_work_instead_of_looping() {
        let root = temp_root("ctox-platform-review-requeue-suppressed");
        channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Kunstmen platform homepage reset".to_string(),
                prompt: "Direct corrective work for the Kunstmen homepage.".to_string(),
                thread_key: "kunstmen-supervisor".to_string(),
                workspace_root: Some("/home/ubuntu/workspace/kunstmen".to_string()),
                priority: "urgent".to_string(),
                suggested_skill: Some("frontend-skill".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to seed direct corrective task");

        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: PLATFORM_IMPLEMENTATION_KIND.to_string(),
                title: "Kunstmen platform implementation reset".to_string(),
                body_text: "Implement the platform front door.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "kunstmen-supervisor",
                    "workspace_root": "/home/ubuntu/workspace/kunstmen",
                    "priority": "high",
                    "skill": "frontend-skill",
                    "dedupe_key": "platform-implementation:kunstmen-supervisor",
                }),
            },
            false,
        )
        .expect("failed to create implementation self-work");

        let queued = requeue_review_rejected_self_work(
            &root,
            &item.work_id,
            "The homepage still does not read like a platform.",
        )
        .expect("review requeue should succeed");
        assert!(queued.is_none());

        let superseded = tickets::load_ticket_self_work_item(&root, &item.work_id)
            .expect("failed to reload self-work")
            .expect("missing self-work");
        assert_eq!(superseded.state, "superseded");
    }

    #[test]
    fn runtime_blocker_backoff_is_visible_in_shared_state() {
        let mut shared = SharedState::default();
        shared.last_error = Some(
            "CTOX chat could not continue because the configured OpenAI API quota is exhausted or billing is unavailable for the selected model.".to_string(),
        );
        shared.last_progress_epoch_secs = current_epoch_secs().saturating_sub(30);

        let remaining =
            runtime_blocker_backoff_remaining_secs(&shared).expect("cooldown should be active");
        assert!(remaining > 0);
        assert!(remaining <= 1_800);
    }

    #[test]
    fn enqueue_prompt_waits_during_hard_runtime_blocker_backoff() {
        let root = temp_root("ctox-enqueue-backoff");
        std::fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("service state poisoned");
            shared.last_error = Some(
                "CTOX chat could not continue because the configured OpenAI API quota is exhausted or billing is unavailable for the selected model.".to_string(),
            );
            shared.last_progress_epoch_secs = current_epoch_secs().saturating_sub(15);
        }

        enqueue_prompt(
            &root,
            &state,
            QueuedPrompt {
                prompt: "Continue mission".to_string(),
                goal: "Continue mission".to_string(),
                preview: "Continue mission".to_string(),
                source_label: "queue".to_string(),
                suggested_skill: None,
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: Vec::new(),
                thread_key: Some("queue/mission-1".to_string()),
                workspace_root: None,
                ticket_self_work_id: None,
                outbound_email: None,
                outbound_anchor: None,
            },
            "Queued queue inbound from CTOX queue".to_string(),
        );

        let shared = state.lock().expect("service state poisoned");
        assert!(!shared.busy);
        assert_eq!(shared.pending_prompts.len(), 1);
        assert!(shared
            .recent_events
            .back()
            .map(|event| event.contains("runtime blocker cooldown"))
            .unwrap_or(false));
        drop(shared);
        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events
            .iter()
            .any(|event| event.mechanism_id == "runtime_blocker_backoff"));
    }

    #[test]
    fn email_prompt_includes_recent_cross_channel_owner_context() {
        let root = std::env::temp_dir().join(format!(
            "ctox-owner-context-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("failed to create temp root");

        channels::handle_channel_command(
            &root,
            &[
                "ingest-tui".to_string(),
                "--account-key".to_string(),
                "tui:local".to_string(),
                "--thread-key".to_string(),
                "tui/main".to_string(),
                "--subject".to_string(),
                "TUI input".to_string(),
                "--sender-display".to_string(),
                "Test Owner".to_string(),
                "--sender-address".to_string(),
                "tui:local".to_string(),
                "--body".to_string(),
                "Die Freigabe fuer Nextcloud wurde im TUI erteilt.".to_string(),
            ],
        )
        .expect("failed to ingest tui message");

        let message = channels::RoutedInboundMessage {
            message_key: "mail-1".to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "email/thread-1".to_string(),
            sender_display: "Test Owner".to_string(),
            sender_address: "michael.welsch@example.com".to_string(),
            subject: "Status?".to_string(),
            preview: "Wie ist der Stand?".to_string(),
            body_text: "Wie ist der Stand?".to_string(),
            external_created_at: "2026-03-26T01:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };

        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@example.com".to_string(),
        );
        settings.insert(
            "CTOX_ALLOWED_EMAIL_DOMAIN".to_string(),
            "example.com".to_string(),
        );
        settings.insert(
            "CTOX_EMAIL_ADMIN_POLICIES".to_string(),
            "opsadmin@example.com:sudo".to_string(),
        );

        let prompt = enrich_inbound_prompt(&root, &settings, &message, "Wie ist der Stand?");
        assert!(prompt.contains("[Kommunikationskontext aktiv pruefen]"));
        assert!(prompt.contains("ctox channel context"));
        assert!(prompt.contains("ctox channel history"));
        assert!(prompt.contains("ctox channel search"));
        assert!(prompt.contains("ctox lcm-grep"));
        assert!(prompt.contains("[E-Mail Berechtigung]"));
        assert!(prompt.contains("email/thread-1"));
        assert!(prompt.contains(
            "Dein gesamter Assistenten-Output in diesem Run ist exakt der zu versendende Mailtext"
        ));
        assert!(prompt.contains("keine Queue-/Review-/Runtime-Sprache"));
    }

    #[test]
    fn resolve_scrape_api_payload_exposes_target_api_latest_and_filtered_records() {
        let root = temp_root("scrape-api");
        let target_path = root.join("target.json");
        let script_path = root.join("root.js");
        let source_a = root.join("source-a.js");
        let source_b = root.join("source-b.js");

        std::fs::write(
            &target_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "target_key": "service-fixture",
                "display_name": "Service Fixture",
                "start_url": "https://example.test/root",
                "target_kind": "articles",
                "config": {
                    "skip_probe": true,
                    "record_key_fields": ["source_key", "url"],
                    "sources": [
                        {
                            "source_key": "source-a",
                            "display_name": "Source A",
                            "start_url": "https://example.test/a",
                            "source_kind": "fixture",
                            "extraction_module": "sources/source-a/extractor.js"
                        },
                        {
                            "source_key": "source-b",
                            "display_name": "Source B",
                            "start_url": "https://example.test/b",
                            "source_kind": "fixture",
                            "extraction_module": "sources/source-b/extractor.js"
                        }
                    ]
                },
                "output_schema": {
                    "schema_key": "articles.v1",
                    "record_key_fields": ["source_key", "url"]
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            &script_path,
            r#"process.stdout.write(JSON.stringify({
  records: [
    {
      source_key: "source-a",
      source: { source_key: "source-a", display_name: "Source A" },
      title: "Alpha",
      url: "https://example.test/a/alpha"
    },
    {
      source_key: "source-b",
      source: { source_key: "source-b", display_name: "Source B" },
      title: "Beta",
      url: "https://example.test/b/beta"
    }
  ]
}, null, 2));"#,
        )
        .unwrap();
        std::fs::write(
            &source_a,
            "module.exports = async function extractSource() { return { records: [] }; };\n",
        )
        .unwrap();
        std::fs::write(
            &source_b,
            "module.exports = async function extractSource() { return { records: [] }; };\n",
        )
        .unwrap();

        scrape::handle_scrape_command(
            &root,
            &[
                "upsert-target".to_string(),
                "--input".to_string(),
                target_path.to_string_lossy().to_string(),
            ],
        )
        .unwrap();
        scrape::handle_scrape_command(
            &root,
            &[
                "register-script".to_string(),
                "--target-key".to_string(),
                "service-fixture".to_string(),
                "--script-file".to_string(),
                script_path.to_string_lossy().to_string(),
                "--change-reason".to_string(),
                "fixture".to_string(),
            ],
        )
        .unwrap();
        scrape::handle_scrape_command(
            &root,
            &[
                "register-source-module".to_string(),
                "--target-key".to_string(),
                "service-fixture".to_string(),
                "--source-key".to_string(),
                "source-a".to_string(),
                "--module-file".to_string(),
                source_a.to_string_lossy().to_string(),
                "--change-reason".to_string(),
                "fixture".to_string(),
            ],
        )
        .unwrap();
        scrape::handle_scrape_command(
            &root,
            &[
                "register-source-module".to_string(),
                "--target-key".to_string(),
                "service-fixture".to_string(),
                "--source-key".to_string(),
                "source-b".to_string(),
                "--module-file".to_string(),
                source_b.to_string_lossy().to_string(),
                "--change-reason".to_string(),
                "fixture".to_string(),
            ],
        )
        .unwrap();
        scrape::handle_scrape_command(
            &root,
            &[
                "execute".to_string(),
                "--target-key".to_string(),
                "service-fixture".to_string(),
            ],
        )
        .unwrap();

        let (api_status, api_payload) =
            resolve_scrape_api_payload(&root, "/ctox/scrape/targets/service-fixture/api").unwrap();
        assert_eq!(api_status, 200);
        assert_eq!(
            api_payload
                .get("source_count")
                .and_then(serde_json::Value::as_u64),
            Some(2)
        );
        assert_eq!(
            api_payload
                .get("source_modules")
                .and_then(serde_json::Value::as_array)
                .map(|items| items.len()),
            Some(2)
        );

        let (latest_status, latest_payload) =
            resolve_scrape_api_payload(&root, "/ctox/scrape/targets/service-fixture/latest")
                .unwrap();
        assert_eq!(latest_status, 200);
        assert_eq!(
            latest_payload
                .get("active_record_count")
                .and_then(serde_json::Value::as_i64),
            Some(2)
        );

        let (records_status, records_payload) = resolve_scrape_api_payload(
            &root,
            "/ctox/scrape/targets/service-fixture/records?source_key=source-a&limit=5",
        )
        .unwrap();
        assert_eq!(records_status, 200);
        assert_eq!(
            records_payload
                .get("count")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            records_payload["items"][0]["record"]["source_key"].as_str(),
            Some("source-a")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_lifecycle_alerts_report_stale_service_pid_file() {
        let root = temp_root("service-alerts-stale-pid");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::fs::write(service_pid_path(&root), "999999\n").unwrap();

        let alerts = runtime_lifecycle_alerts(&root, None, false).unwrap();

        assert!(alerts
            .iter()
            .any(|alert| alert.contains("stale service pid file")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_lifecycle_alerts_report_backend_residue() {
        let root = temp_root("service-alerts-backend-residue");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::fs::write(root.join("runtime/ctox_chat_backend.pid"), "999999\n").unwrap();

        let alerts = runtime_lifecycle_alerts(&root, None, false).unwrap();

        assert!(alerts
            .iter()
            .any(|alert| alert.contains("backend residue stale pid file")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn queued_prompt_without_outbound_email_yields_no_proactive_action() {
        let job = QueuedPrompt {
            prompt: "Please reach out to founder@external.test about the Kunstmen update."
                .to_string(),
            goal: "outreach".to_string(),
            preview: "outreach".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        // No structured intent on the job: post-turn proactive action must be
        // None even though the prompt body name-drops a founder address and
        // mentions "Kunstmen update". This is the deliberate, post-heuristic
        // contract — keyword-scanning is gone.
        assert!(job.outbound_email.is_none());
    }

    #[test]
    fn queued_prompt_with_explicit_outbound_email_clones_through() {
        let intent = channels::FounderOutboundAction {
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "kunstmen-supervisor".to_string(),
            subject: "Operator-supplied Subject".to_string(),
            to: vec!["founder@external.test".to_string()],
            cc: vec!["co@external.test".to_string()],
            attachments: Vec::new(),
        };
        let job = QueuedPrompt {
            prompt: "Body".to_string(),
            goal: "Body".to_string(),
            preview: "Body".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("kunstmen-supervisor".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: Some(intent.clone()),
            outbound_anchor: None,
        };

        let routed = job.outbound_email.clone().expect("outbound_email present");
        assert_eq!(routed.account_key, intent.account_key);
        assert_eq!(routed.thread_key, intent.thread_key);
        assert_eq!(routed.subject, intent.subject);
        assert_eq!(routed.to, intent.to);
        assert_eq!(routed.cc, intent.cc);
    }

    // Anchor wire-up: TUI-initiated proactive outbound has no leased
    // inbound message key, so the post-turn dispatcher would silently
    // skip the reviewed-founder-outbound send. The synthetic anchor
    // (set by `submit_chat_prompt_with_intent`) restores the link.
    #[test]
    fn founder_outbound_anchor_key_prefers_explicit_outbound_anchor() {
        let intent = channels::FounderOutboundAction {
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "chat-outbound".to_string(),
            subject: "Update".to_string(),
            to: vec!["founder@example.test".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
        };
        let job = QueuedPrompt {
            prompt: "Draft a quick update for the founder.".to_string(),
            goal: "Draft a quick update for the founder.".to_string(),
            preview: "preview".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("chat-outbound".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: Some(intent),
            outbound_anchor: Some("tui-outbound:test-id".to_string()),
        };
        assert_eq!(
            founder_outbound_anchor_key(&job),
            Some("tui-outbound:test-id"),
        );
    }

    // Regression guard: without an explicit anchor and no leased inbound
    // message key, the resolver must return None — never invent one from
    // prompt text.
    #[test]
    fn founder_outbound_anchor_key_returns_none_when_unset_and_no_lease() {
        let job = QueuedPrompt {
            prompt: "Reach out to the founder about Kunstmen.".to_string(),
            goal: "Reach out to the founder about Kunstmen.".to_string(),
            preview: "preview".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };
        assert!(founder_outbound_anchor_key(&job).is_none());
    }

    // Inbound-driven jobs (no synthetic anchor) keep the legacy fallback:
    // anchor is the first leased message key.
    #[test]
    fn founder_outbound_anchor_key_falls_back_to_leased_message_key() {
        let job = QueuedPrompt {
            prompt: "Reply to the founder.".to_string(),
            goal: "Reply to the founder.".to_string(),
            preview: "preview".to_string(),
            source_label: "email".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["msg-key-42".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };
        assert_eq!(founder_outbound_anchor_key(&job), Some("msg-key-42"));
    }

    #[test]
    fn outbound_email_intent_round_trips_into_founder_outbound_action() {
        let intent = OutboundEmailIntent {
            account_key: "email:cto1@example.com".to_string(),
            thread_key: "chat-outbound".to_string(),
            subject: "Update".to_string(),
            to: vec!["d.lottes@example.test".to_string()],
            cc: vec!["j.kienzler@example.test".to_string()],
            attachments: Vec::new(),
        };
        let action: channels::FounderOutboundAction = intent.into();
        assert_eq!(action.account_key, "email:cto1@example.com");
        assert_eq!(action.thread_key, "chat-outbound");
        assert_eq!(action.subject, "Update");
        assert_eq!(action.to, vec!["d.lottes@example.test".to_string()]);
        assert_eq!(action.cc, vec!["j.kienzler@example.test".to_string()]);
    }

    #[test]
    fn reviewed_founder_send_prompt_declares_outcome_artifact() {
        let job = QueuedPrompt {
            prompt:
                "Schreibe und sende per reviewed-founder-send eine Mail an j.kienzler@example.test."
                    .to_string(),
            goal: "send mail".to_string(),
            preview: "send mail".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:send-mail".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("thread:julia".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let refs = expected_outcome_artifacts_for_job(&job);

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, ArtifactKind::OutboundEmail);
        assert_eq!(refs[0].primary_key, "thread:thread:julia");
        assert_eq!(refs[0].expected_terminal_state, "accepted");
    }

    #[test]
    fn outcome_witness_blocks_claimed_mail_completion_without_delivery_ref() {
        let root = temp_root("outcome-witness-missing-delivery");
        let job = QueuedPrompt {
            prompt:
                "Schreibe und sende per reviewed-founder-send eine Mail an j.kienzler@example.test."
                    .to_string(),
            goal: "send mail".to_string(),
            preview: "send mail".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:send-mail".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("thread:julia".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let err = enforce_job_outcome_witness(
            &root,
            &job,
            expected_outcome_artifacts_for_job(&job),
            Vec::new(),
        )
        .expect_err("missing outbound artifact must block completion");

        assert!(err.to_string().contains("dauerhafte Ergebnis-Artefakt"));
        let conn = channels::open_channel_db(&crate::paths::core_db(&root))
            .expect("failed to open channel db");
        let rejected_count: i64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM ctox_core_transition_proofs
                WHERE entity_type = 'QueueItem'
                  AND entity_id = 'queue:send-mail'
                  AND to_state = 'Completed'
                  AND accepted = 0
                  AND violation_codes_json LIKE '%WP-Outcome-Missing%'
                "#,
                [],
                |row| row.get(0),
            )
            .expect("failed to count rejected outcome proof");
        assert_eq!(rejected_count, 1);
    }

    #[test]
    fn queue_prompt_declares_required_workspace_file_artifacts() {
        let job = QueuedPrompt {
            prompt: "RUN_DIR=\"/tmp/ctox-tb2-run\"\nInitialisiere die Dateien logbook.md, controller.json, results.jsonl und blogpost-notes.md.".to_string(),
            goal: "bootstrap artifacts".to_string(),
            preview: "bootstrap artifacts".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-bootstrap".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let refs = expected_outcome_artifacts_for_job(&job);
        let paths = refs
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::WorkspaceFile)
            .map(|artifact| artifact.primary_key.as_str())
            .collect::<Vec<_>>();

        assert!(paths.contains(&"/tmp/ctox-tb2-run/logbook.md"));
        assert!(paths.contains(&"/tmp/ctox-tb2-run/controller.json"));
        assert!(paths.contains(&"/tmp/ctox-tb2-run/results.jsonl"));
        assert!(paths.contains(&"/tmp/ctox-tb2-run/blogpost-notes.md"));
        assert!(refs
            .iter()
            .all(|artifact| artifact.expected_terminal_state == "present"));
    }

    #[test]
    fn queue_prompt_declares_smoke_workspace_file_artifact() {
        let job = QueuedPrompt {
            prompt: "RUN_DIR=\"/tmp/ctox-smoke\". Initialisiere die Datei required-smoke.json."
                .to_string(),
            goal: "smoke artifact".to_string(),
            preview: "smoke artifact".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:smoke-artifact".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let refs = expected_outcome_artifacts_for_job(&job);
        assert!(refs.iter().any(|artifact| {
            artifact.kind == ArtifactKind::WorkspaceFile
                && artifact.primary_key == "/tmp/ctox-smoke/required-smoke.json"
                && artifact.expected_terminal_state == "present"
        }));
    }

    #[test]
    fn chat_prompt_declares_workspace_relative_smoke_artifact() {
        let run_dir = "/tmp/ctox-model-smoke/20260506T195937-hy3-responses-id-smoke";
        let job = QueuedPrompt {
            prompt: format!(
                "Work only inside this workspace: {run_dir}\n\
Create a file named smoke.txt inside that workspace containing exactly HY3_CTOX_OK.\n\
Use shell tools and verify with `test -f {run_dir}/smoke.txt` before claiming completion."
            ),
            goal: "HY3 smoke artifact".to_string(),
            preview: "HY3 smoke artifact".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("smoke/hy3-responses-id".to_string()),
            workspace_root: Some(run_dir.to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let refs = expected_outcome_artifacts_for_job(&job);
        assert!(refs.iter().any(|artifact| {
            artifact.kind == ArtifactKind::WorkspaceFile
                && artifact.primary_key == format!("{run_dir}/smoke.txt")
                && artifact.expected_terminal_state == "present"
        }));

        let prompt = artifact_first_execution_prompt(&job);
        assert!(prompt.contains("HARNESS ARTIFACT CONTRACT"));
        assert!(prompt.contains("Workspace root:"));
        assert!(prompt.contains(run_dir));
        assert!(prompt.contains("install directory"));
        assert!(prompt.contains(&format!("{run_dir}/smoke.txt")));
    }

    #[test]
    fn outcome_witness_blocks_hy3_smoke_when_file_written_in_wrong_directory() {
        let root = temp_root("outcome-witness-hy3-smoke-wrong-directory");
        let run_dir = root.join("model-smoke/hy3");
        let wrong_dir = root.join("install-current");
        std::fs::create_dir_all(&wrong_dir).expect("failed to create wrong dir");
        std::fs::write(wrong_dir.join("smoke.txt"), "HY3_CTOX_OK\n")
            .expect("failed to write wrong smoke file");
        let job = QueuedPrompt {
            prompt: format!(
                "Work only inside this workspace: {}\n\
Create a file named smoke.txt inside that workspace containing exactly HY3_CTOX_OK.",
                run_dir.display()
            ),
            goal: "HY3 smoke artifact".to_string(),
            preview: "HY3 smoke artifact".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("smoke/hy3-wrong-dir".to_string()),
            workspace_root: Some(run_dir.to_string_lossy().into_owned()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let expected = expected_outcome_artifacts_for_job(&job);
        assert_eq!(
            expected
                .iter()
                .filter(|artifact| artifact.kind == ArtifactKind::WorkspaceFile)
                .count(),
            1
        );
        let delivered = delivered_outcome_artifacts_for_job(&root, &job, &expected)
            .expect("failed to inspect delivered artifacts");
        assert!(delivered.is_empty());
        let err = enforce_job_outcome_witness(&root, &job, expected, delivered)
            .expect_err("wrong-directory smoke file must not satisfy witness");
        assert!(err.to_string().contains("dauerhafte Ergebnis-Artefakt"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_required_durable_files_section_limits_workspace_artifacts() {
        let run_dir = "/tmp/ctox-tb2-run";
        let job = QueuedPrompt {
            prompt: format!(
                "Runtime requirements:\n- record context_window in summary.md.\n\n\
Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/run-log.md\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/summary.md\n\n\
Initial completion criteria:\n\
- controller.json records the model.\n\
- ticket-map.jsonl contains preparation tickets.\n\
- run-log.md records planning.\n\
- results.jsonl records outcomes.\n\
- summary.md states next action.\n\
- helper files like controller-prompt.md and runtime-switch.json may exist but are not required durable files."
            ),
            goal: "Terminal-Bench 2 controller".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-controller".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let refs = expected_outcome_artifacts_for_job(&job);
        let paths = refs
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::WorkspaceFile)
            .map(|artifact| artifact.primary_key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                "/tmp/ctox-tb2-run/controller.json",
                "/tmp/ctox-tb2-run/ticket-map.jsonl",
                "/tmp/ctox-tb2-run/run-log.md",
                "/tmp/ctox-tb2-run/results.jsonl",
                "/tmp/ctox-tb2-run/summary.md",
            ]
        );
    }

    #[test]
    fn durable_artifact_contract_section_limits_workspace_artifacts() {
        let run_dir = "/tmp/ctox-tb2-run";
        let job = QueuedPrompt {
            prompt: format!(
                "DURABLE ARTIFACT CONTRACT\n\
Create these five files immediately:\n\
1. {run_dir}/controller.json\n\
2. {run_dir}/ticket-map.jsonl\n\
3. {run_dir}/run-log.md\n\
4. {run_dir}/results.jsonl\n\
5. {run_dir}/summary.md\n\n\
Write {run_dir}/controller.json as valid JSON after planning. Helper files like {run_dir}/controller-prompt.md may exist but are not required."
            ),
            goal: "Terminal-Bench 2 controller".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-controller".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let refs = expected_outcome_artifacts_for_job(&job);
        let paths = refs
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::WorkspaceFile)
            .map(|artifact| artifact.primary_key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                "/tmp/ctox-tb2-run/controller.json",
                "/tmp/ctox-tb2-run/ticket-map.jsonl",
                "/tmp/ctox-tb2-run/run-log.md",
                "/tmp/ctox-tb2-run/results.jsonl",
                "/tmp/ctox-tb2-run/summary.md",
            ]
        );
    }

    #[test]
    fn artifact_first_prompt_front_loads_declared_workspace_files() {
        let run_dir = "/tmp/terminal-bench-2/runs/20260506T104258Z-qwen36-128k-worker-owned-queue";
        let job = QueuedPrompt {
            prompt: format!(
                "Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/summary.md\n\n\
Start by discovering benchmark tasks."
            ),
            goal: "Terminal-Bench 2 controller".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-controller".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let prompt = artifact_first_execution_prompt(&job);

        assert!(prompt.starts_with("HARNESS TERMINAL-BENCH PREFLIGHT"));
        assert!(prompt.contains("The worker must perform this preflight itself"));
        assert!(prompt.contains("The harness and review system will not create files"));
        assert!(prompt.contains("CURRENT TERMINAL-BENCH RUN SCOPE"));
        assert!(prompt.contains("Use exactly this RUN_DIR"));
        assert!(prompt.contains(run_dir));
        assert!(prompt.contains("controller-prompt.md"));
        assert!(prompt.contains("wrong-run error"));
        assert!(prompt.contains("FIRST TOOL CALL CONTRACT"));
        assert!(prompt.contains(&format!("{run_dir}/controller.json")));
        assert!(prompt.contains(&format!("{run_dir}/summary.md")));
        assert!(prompt.contains("regular file"));
        assert!(prompt.contains("test -f"));
        assert!(prompt.contains("Forbidden before the above succeeds"));
        assert!(prompt.contains("Original controller task is intentionally withheld"));
        assert!(!prompt.contains("Start by discovering benchmark tasks"));
        assert!(!prompt.contains("ORIGINAL TASK"));
    }

    #[test]
    fn terminal_bench_preflight_spec_uses_current_job_artifacts() {
        let run_dir = "/tmp/terminal-bench-2/runs/20260506T122406Z-qwen36-128k-40965ff-clean2";
        let stale_run_dir = "/tmp/terminal-bench-2/runs/20260506T120350Z-stale";
        let job = QueuedPrompt {
            prompt: format!(
                "Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/preparation-tickets.jsonl\n\
- {run_dir}/run-queue.jsonl\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md\n\
- {run_dir}/blogpost-notes.md\n\n\
Stale context may mention {stale_run_dir}/controller.json, but do not use it.\n\
Create durable CTOX queue/ticket work and record message keys."
            ),
            goal: "Terminal-Bench 2 controller".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-controller".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let spec = terminal_bench_preflight_spec_for_job(&job).unwrap();

        assert_eq!(spec.run_dir, run_dir);
        assert!(spec.requires_runtime_refs);
        assert_eq!(spec.required_files.len(), 8);
        assert!(spec
            .required_files
            .iter()
            .all(|path| path.starts_with(run_dir)));
        assert!(!spec
            .required_files
            .iter()
            .any(|path| path.contains(stale_run_dir)));
    }

    #[test]
    fn short_terminal_bench_reply_holds_artifact_job_open() {
        let root = temp_root("terminal-bench-short-reply-hold");
        let run_dir = root.join("terminal-bench-2/runs/short-reply");
        let run_dir = run_dir.to_string_lossy().into_owned();
        let job = QueuedPrompt {
            prompt: format!(
                "Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/preparation-tickets.jsonl\n\
- {run_dir}/run-queue.jsonl\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md\n\
- {run_dir}/blogpost-notes.md\n\n\
The controller must create preparation queue/tickets and record queue:system::* keys."
            ),
            goal: "Terminal-Bench 2 controller".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::parent".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("terminal-bench-2/deepseek/short-reply/controller".to_string()),
            workspace_root: Some(run_dir.clone()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let disposition = short_terminal_bench_artifact_reply_disposition(&root, &job, "::")
            .expect("short artifact reply must hold the slice open");

        assert!(matches!(
            disposition,
            CompletionReviewDisposition::Hold { .. }
        ));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn terminal_bench_review_feedback_retries_parent_queue_before_threshold() {
        let root = temp_root("terminal-bench-feedback-parent-retry");
        let run_dir = root.join("terminal-bench-2/runs/feedback-retry");
        let run_dir = run_dir.to_string_lossy().into_owned();
        let job = QueuedPrompt {
            prompt: format!(
                "HARNESS FEEDBACK\n\
Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/preparation-tickets.jsonl\n\
- {run_dir}/run-queue.jsonl\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md\n\
- {run_dir}/blogpost-notes.md\n\n\
The controller must create preparation queue/tickets and record queue:system::* keys."
            ),
            goal: "Continue Terminal-Bench controller".to_string(),
            preview: "Terminal-Bench review feedback".to_string(),
            source_label: "review-feedback".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::parent".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("terminal-bench-2/deepseek/feedback-retry/controller".to_string()),
            workspace_root: Some(run_dir),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert_eq!(
            outcome_witness_retry_route_status_for_job(&root, &job),
            "pending"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn terminal_bench_review_feedback_keeps_retrying_after_outcome_rejections() {
        let root = temp_root("terminal-bench-feedback-no-circuit-block");
        let run_dir = root.join("terminal-bench-2/runs/feedback-no-circuit-block");
        let run_dir = run_dir.to_string_lossy().into_owned();
        let job = QueuedPrompt {
            prompt: format!(
                "HARNESS FEEDBACK\n\
Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md\n\n\
The controller must update stale files itself."
            ),
            goal: "Continue Terminal-Bench controller".to_string(),
            preview: "Terminal-Bench review feedback".to_string(),
            source_label: "review-feedback".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::parent-no-circuit".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some(
                "terminal-bench-2/deepseek/feedback-no-circuit/controller".to_string(),
            ),
            workspace_root: Some(run_dir),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        for _ in 0..=review_checkpoint_requeue_block_threshold() {
            let _ = enforce_job_outcome_witness(
                &root,
                &job,
                vec![ArtifactRef {
                    kind: ArtifactKind::WorkspaceFile,
                    primary_key: "/missing/controller.json".to_string(),
                    expected_terminal_state: "fresh".to_string(),
                }],
                Vec::new(),
            );
        }

        assert_eq!(
            outcome_witness_retry_route_status_for_job(&root, &job),
            "pending"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn terminal_bench_controller_artifact_job_does_not_circuit_block() {
        let root = temp_root("terminal-bench-controller-no-circuit-block");
        let run_dir = root.join("terminal-bench-2/runs/controller-no-circuit-block");
        let run_dir = run_dir.to_string_lossy().into_owned();
        let job = QueuedPrompt {
            prompt: format!(
                "Terminal-Bench controller artifact contract:\n\
- {run_dir}/controller.json\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md"
            ),
            goal: "Terminal-Bench 2 controller".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::controller-no-circuit".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some(
                "terminal-bench-2/deepseek/controller-no-circuit/controller".to_string(),
            ),
            workspace_root: Some(run_dir),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        for _ in 0..=review_checkpoint_requeue_block_threshold() {
            let _ = enforce_job_outcome_witness(
                &root,
                &job,
                vec![ArtifactRef {
                    kind: ArtifactKind::WorkspaceFile,
                    primary_key: "/missing/controller.json".to_string(),
                    expected_terminal_state: "fresh".to_string(),
                }],
                Vec::new(),
            );
        }

        assert_eq!(
            outcome_witness_retry_route_status_for_job(&root, &job),
            "review_rework"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn terminal_bench_review_feedback_does_not_need_outcome_recovery_prompt() {
        let root = temp_root("terminal-bench-feedback-no-recovery-prompt");
        let run_dir = root.join("terminal-bench-2/runs/feedback-no-recovery");
        let run_dir = run_dir.to_string_lossy().into_owned();
        let job = QueuedPrompt {
            prompt: format!(
                "HARNESS FEEDBACK\n\
Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/preparation-tickets.jsonl\n\
- {run_dir}/run-queue.jsonl\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md\n\
- {run_dir}/blogpost-notes.md\n\n\
The controller must create preparation queue/tickets and record queue:system::* keys."
            ),
            goal: "Continue Terminal-Bench controller".to_string(),
            preview: "Terminal-Bench review feedback".to_string(),
            source_label: "review-feedback".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::parent".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some(
                "terminal-bench-2/deepseek/feedback-no-recovery/controller".to_string(),
            ),
            workspace_root: Some(run_dir),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert!(is_terminal_bench_controller_artifact_job(&job));
        assert_eq!(
            outcome_witness_retry_route_status_for_job(&root, &job),
            "pending"
        );
        assert!(!should_queue_artifact_outcome_recovery(&job));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn terminal_bench_preflight_spec_stops_after_real_bootstrap_refs_exist() {
        let root = temp_root("terminal-bench-bootstrap-refs-skip-first-guard");
        let run_dir = root.join("terminal-bench-2/runs/run-with-real-refs");
        std::fs::create_dir_all(run_dir.join("tasks")).expect("failed to create tasks dir");
        let files = [
            "controller.json",
            "ticket-map.jsonl",
            "preparation-tickets.jsonl",
            "run-queue.jsonl",
            "results.jsonl",
            "knowledge.md",
            "logbook.md",
            "blogpost-notes.md",
        ];
        for file in files {
            let content = match file {
                "ticket-map.jsonl" | "preparation-tickets.jsonl" | "run-queue.jsonl" => {
                    "{\"message_key\":\"queue:system::abc123\"}\n"
                }
                "results.jsonl" => "",
                "controller.json" => "{\"status\":\"preflight\"}\n",
                _ => "# status\n",
            };
            std::fs::write(run_dir.join(file), content).expect("failed to write artifact");
        }
        let run_dir = run_dir.to_string_lossy().into_owned();
        let job = QueuedPrompt {
            prompt: format!(
                "Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/preparation-tickets.jsonl\n\
- {run_dir}/run-queue.jsonl\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md\n\
- {run_dir}/blogpost-notes.md\n\n\
The controller must create preparation queue/tickets and record queue:system::* keys.\n\
Continue by fixing results.jsonl."
            ),
            goal: "Terminal-Bench 2 controller".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec!["queue:system::parent".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("terminal-bench-2/deepseek/run-with-real-refs/controller".to_string()),
            workspace_root: Some(run_dir.clone()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert!(is_terminal_bench_controller_artifact_job(&job));
        assert!(terminal_bench_preflight_spec_for_job(&job).is_none());
        let prompt = artifact_first_execution_prompt(&job);
        assert!(prompt.starts_with("HARNESS ARTIFACT CONTRACT"));
        assert!(prompt.contains("ORIGINAL TASK"));
        assert!(prompt.contains("Continue by fixing results.jsonl"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn terminal_bench_review_feedback_does_not_reenter_first_turn_preflight() {
        let run_dir = "/tmp/terminal-bench-2/runs/review-feedback-run";
        let job = QueuedPrompt {
            prompt: format!(
                "HARNESS FEEDBACK\n\
The previous controller turn is incomplete. Update these files now:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/preparation-tickets.jsonl\n\
- {run_dir}/run-queue.jsonl\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md\n\
- {run_dir}/blogpost-notes.md\n"
            ),
            goal: "Address Terminal-Bench review feedback".to_string(),
            preview: "Terminal-Bench review feedback".to_string(),
            source_label: "review-feedback".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("terminal-bench-2/review-feedback".to_string()),
            workspace_root: Some(run_dir.to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert!(is_terminal_bench_controller_artifact_job(&job));
        assert!(terminal_bench_preflight_spec_for_job(&job).is_none());
        assert!(completion_review_should_skip_feedback_turn(&job));
        let prompt = artifact_first_execution_prompt(&job);
        assert!(!prompt.starts_with("HARNESS TERMINAL-BENCH PREFLIGHT"));
        assert!(prompt.contains("ORIGINAL TASK"));
        assert!(prompt.contains("The previous controller turn is incomplete"));
    }

    #[test]
    fn artifact_first_prompt_keeps_original_task_for_generic_artifact_jobs() {
        let run_dir = "/tmp/ctox-generic-artifacts";
        let job = QueuedPrompt {
            prompt: format!(
                "Only required durable files for this worker turn:\n\
- {run_dir}/summary.md\n\n\
Start by checking the local service status."
            ),
            goal: "generic artifact worker".to_string(),
            preview: "generic artifact worker".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:generic-artifact".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let prompt = artifact_first_execution_prompt(&job);

        assert!(prompt.starts_with("HARNESS ARTIFACT CONTRACT"));
        assert!(prompt.contains("ORIGINAL TASK"));
        assert!(prompt.contains("Start by checking the local service status"));
        assert!(prompt.contains(&format!("{run_dir}/summary.md")));
    }

    #[test]
    fn artifact_first_prompt_leaves_non_artifact_jobs_unchanged() {
        let job = QueuedPrompt {
            prompt: "Summarize the current service status.".to_string(),
            goal: "status".to_string(),
            preview: "status".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:status".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        assert_eq!(artifact_first_execution_prompt(&job), job.prompt);
    }

    #[test]
    fn outcome_witness_blocks_queue_completion_without_workspace_file_artifact() {
        let root = temp_root("outcome-witness-missing-workspace-file");
        let run_dir = root.join("tb2-run");
        let job = QueuedPrompt {
            prompt: format!(
                "RUN_DIR=\"{}\"\nInitialisiere die Dateien logbook.md und controller.json.",
                run_dir.display()
            ),
            goal: "bootstrap artifacts".to_string(),
            preview: "bootstrap artifacts".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-bootstrap".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let err = enforce_job_outcome_witness(
            &root,
            &job,
            expected_outcome_artifacts_for_job(&job),
            Vec::new(),
        )
        .expect_err("missing workspace file artifact must block queue completion");

        assert!(err.to_string().contains("dauerhafte Ergebnis-Artefakt"));
        let conn = channels::open_channel_db(&crate::paths::core_db(&root))
            .expect("failed to open channel db");
        let rejected_count: i64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM ctox_core_transition_proofs
                WHERE entity_type = 'QueueItem'
                  AND entity_id = 'queue:tb2-bootstrap'
                  AND to_state = 'Completed'
                  AND accepted = 0
                  AND violation_codes_json LIKE '%WP-Outcome-Missing%'
                "#,
                [],
                |row| row.get(0),
            )
            .expect("failed to count rejected outcome proof");
        assert_eq!(rejected_count, 1);
    }

    #[test]
    fn outcome_witness_accepts_present_workspace_file_artifacts() {
        let root = temp_root("outcome-witness-present-workspace-file");
        let run_dir = root.join("tb2-run");
        std::fs::create_dir_all(&run_dir).expect("failed to create run dir");
        std::fs::write(run_dir.join("logbook.md"), "# log\n").expect("failed to write logbook");
        std::fs::write(run_dir.join("controller.json"), "{}\n")
            .expect("failed to write controller");
        let job = QueuedPrompt {
            prompt: format!(
                "RUN_DIR=\"{}\"\nInitialisiere die Dateien logbook.md und controller.json.",
                run_dir.display()
            ),
            goal: "bootstrap artifacts".to_string(),
            preview: "bootstrap artifacts".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-bootstrap".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };
        let expected = expected_outcome_artifacts_for_job(&job);
        let delivered = delivered_outcome_artifacts_for_job(&root, &job, &expected)
            .expect("failed to read delivered artifacts");

        let proof_id = enforce_job_outcome_witness(&root, &job, expected, delivered)
            .expect("present file artifacts should satisfy witness")
            .expect("proof id should be returned");

        assert!(proof_id.starts_with("ctp-"));
    }

    #[test]
    fn checkpoint_workspace_artifacts_require_fresh_delivery_after_queue_lease() {
        let root = temp_root("outcome-witness-stale-checkpoint-file");
        let run_dir = root.join("tb2-run");
        std::fs::create_dir_all(&run_dir).expect("failed to create run dir");
        let controller = run_dir.join("controller.json");
        let logbook = run_dir.join("logbook.md");
        std::fs::write(&controller, "{}\n").expect("failed to write stale controller");
        std::fs::write(&logbook, "# old\n").expect("failed to write stale logbook");
        std::thread::sleep(Duration::from_secs(2));
        let prompt = format!(
            "CHECKPOINT-ONLY TERMINAL-BENCH RECOVERY SLICE\n\
Required output files to update now:\n\
- {}\n\
- {}\n\
Exit after the write command.",
            controller.display(),
            logbook.display()
        );
        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "checkpoint stale".to_string(),
                prompt: prompt.clone(),
                thread_key: "queue/checkpoint-stale".to_string(),
                workspace_root: Some(run_dir.to_string_lossy().into_owned()),
                priority: "urgent".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create queue task");
        let leased = channels::lease_queue_task(&root, &task.message_key, "test")
            .expect("failed to lease queue task");
        let job = QueuedPrompt {
            prompt,
            goal: "checkpoint stale".to_string(),
            preview: "checkpoint stale".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec![leased.message_key],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/checkpoint-stale".to_string()),
            workspace_root: Some(run_dir.to_string_lossy().into_owned()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let expected = expected_outcome_artifacts_for_job(&job);
        assert!(expected.iter().all(|artifact| {
            artifact.kind != ArtifactKind::WorkspaceFile
                || artifact.expected_terminal_state == "fresh"
        }));
        let delivered = delivered_outcome_artifacts_for_job(&root, &job, &expected)
            .expect("failed to read delivered artifacts");

        assert!(delivered.is_empty());
        let err = enforce_job_outcome_witness(&root, &job, expected, delivered)
            .expect_err("stale checkpoint files must not satisfy fresh outcome witness");
        assert!(err.to_string().contains("dauerhafte Ergebnis-Artefakt"));
    }

    #[test]
    fn checkpoint_workspace_artifacts_accept_fresh_delivery_after_queue_lease() {
        let root = temp_root("outcome-witness-fresh-checkpoint-file");
        let run_dir = root.join("tb2-run");
        let controller = run_dir.join("controller.json");
        let logbook = run_dir.join("logbook.md");
        let prompt = format!(
            "CHECKPOINT-ONLY TERMINAL-BENCH RECOVERY SLICE\n\
Required output files to update now:\n\
- {}\n\
- {}\n\
Exit after the write command.",
            controller.display(),
            logbook.display()
        );
        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "checkpoint fresh".to_string(),
                prompt: prompt.clone(),
                thread_key: "queue/checkpoint-fresh".to_string(),
                workspace_root: Some(run_dir.to_string_lossy().into_owned()),
                priority: "urgent".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create queue task");
        let leased = channels::lease_queue_task(&root, &task.message_key, "test")
            .expect("failed to lease queue task");
        std::fs::create_dir_all(&run_dir).expect("failed to create run dir");
        std::fs::write(&controller, "{\"phase\":\"checkpoint\"}\n")
            .expect("failed to write fresh controller");
        std::fs::write(&logbook, "# checkpoint\n").expect("failed to write fresh logbook");
        let job = QueuedPrompt {
            prompt,
            goal: "checkpoint fresh".to_string(),
            preview: "checkpoint fresh".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec![leased.message_key],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/checkpoint-fresh".to_string()),
            workspace_root: Some(run_dir.to_string_lossy().into_owned()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };
        let expected = expected_outcome_artifacts_for_job(&job);
        let delivered = delivered_outcome_artifacts_for_job(&root, &job, &expected)
            .expect("failed to read delivered artifacts");

        assert_eq!(delivered.len(), 2);
        let proof_id = enforce_job_outcome_witness(&root, &job, expected, delivered)
            .expect("fresh checkpoint files should satisfy witness")
            .expect("proof id should be returned");
        assert!(proof_id.starts_with("ctp-"));
    }

    #[test]
    fn terminal_bench_controller_rejects_synthetic_runtime_ticket_refs() {
        let root = temp_root("terminal-bench-synthetic-runtime-refs");
        let run_dir = root.join("tb2-run");
        let controller = run_dir.join("controller.json");
        let ticket_map = run_dir.join("ticket-map.jsonl");
        let preparation = run_dir.join("preparation-tickets.jsonl");
        let run_queue = run_dir.join("run-queue.jsonl");
        let prompt = format!(
            "You are CTOX running the Terminal-Bench 2 evaluation controller.\n\
REQUIRED OUTPUT FILES TO UPDATE NOW\n\
- {}\n\
- {}\n\
- {}\n\
- {}\n\n\
Before any benchmark task attempt, create durable CTOX queue/ticket work for \
preparation queue/tickets and record message keys.",
            controller.display(),
            ticket_map.display(),
            preparation.display(),
            run_queue.display()
        );
        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Terminal-Bench 2 controller synthetic refs".to_string(),
                prompt: prompt.clone(),
                thread_key: "queue/tb2-synthetic-refs".to_string(),
                workspace_root: Some(run_dir.to_string_lossy().into_owned()),
                priority: "urgent".to_string(),
                suggested_skill: Some("benchmark-controller".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create parent queue task");
        let leased = channels::lease_queue_task(&root, &task.message_key, "test")
            .expect("failed to lease parent queue task");
        std::fs::create_dir_all(&run_dir).expect("failed to create run dir");
        std::fs::write(
            &controller,
            "{\"phase\":\"1-preparation\",\"next_action\":\"prep\"}\n",
        )
        .expect("failed to write controller");
        std::fs::write(
            &ticket_map,
            "{\"ticket_id\":\"prep-runtime\",\"message_key\":\"msg-prep-runtime-001\"}\n",
        )
        .expect("failed to write ticket map");
        std::fs::write(
            &preparation,
            "{\"ticket_id\":\"prep-runtime\",\"message_key\":\"msg-prep-runtime-001\"}\n",
        )
        .expect("failed to write prep tickets");
        std::fs::write(
            &run_queue,
            "{\"queue_id\":\"q1\",\"ticket_id\":\"prep-runtime\",\"status\":\"pending\"}\n",
        )
        .expect("failed to write run queue");
        let job = QueuedPrompt {
            prompt,
            goal: "Terminal-Bench 2 controller".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec![leased.message_key],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/tb2-synthetic-refs".to_string()),
            workspace_root: Some(run_dir.to_string_lossy().into_owned()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let expected = expected_outcome_artifacts_for_job(&job);
        let delivered = delivered_outcome_artifacts_for_job(&root, &job, &expected)
            .expect("failed to read delivered artifacts");

        assert_eq!(delivered.len(), 4);
        let err = enforce_job_outcome_witness(&root, &job, expected, delivered)
            .expect_err("synthetic runtime refs must not satisfy Terminal-Bench witness");
        assert!(err
            .to_string()
            .contains("missing real CTOX runtime ticket/queue refs"));
        assert!(err.to_string().contains("msg-prep-runtime-001"));
    }

    #[test]
    fn terminal_bench_controller_accepts_real_queue_runtime_refs() {
        let root = temp_root("terminal-bench-real-runtime-refs");
        let run_dir = root.join("tb2-run");
        let controller = run_dir.join("controller.json");
        let ticket_map = run_dir.join("ticket-map.jsonl");
        let preparation = run_dir.join("preparation-tickets.jsonl");
        let run_queue = run_dir.join("run-queue.jsonl");
        let prompt = format!(
            "You are CTOX running the Terminal-Bench 2 evaluation controller.\n\
REQUIRED OUTPUT FILES TO UPDATE NOW\n\
- {}\n\
- {}\n\
- {}\n\
- {}\n\n\
Before any benchmark task attempt, create durable CTOX queue/ticket work for \
preparation queue/tickets and record message keys.",
            controller.display(),
            ticket_map.display(),
            preparation.display(),
            run_queue.display()
        );
        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Terminal-Bench 2 controller real refs".to_string(),
                prompt: prompt.clone(),
                thread_key: "queue/tb2-real-refs".to_string(),
                workspace_root: Some(run_dir.to_string_lossy().into_owned()),
                priority: "urgent".to_string(),
                suggested_skill: Some("benchmark-controller".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create parent queue task");
        let leased = channels::lease_queue_task(&root, &task.message_key, "test")
            .expect("failed to lease parent queue task");
        let child = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "prep-runtime".to_string(),
                prompt: "verify runtime".to_string(),
                thread_key: "queue/tb2-real-refs/prep-runtime".to_string(),
                workspace_root: Some(run_dir.to_string_lossy().into_owned()),
                priority: "urgent".to_string(),
                suggested_skill: Some("benchmark-controller".to_string()),
                parent_message_key: Some(leased.message_key.clone()),
                extra_metadata: None,
            },
        )
        .expect("failed to create child queue task");
        std::fs::create_dir_all(&run_dir).expect("failed to create run dir");
        std::fs::write(
            &controller,
            "{\"phase\":\"1-preparation\",\"next_action\":\"prep\"}\n",
        )
        .expect("failed to write controller");
        std::fs::write(
            &ticket_map,
            format!(
                "{{\"ticket_id\":\"prep-runtime\",\"message_key\":\"{}\"}}\n",
                child.message_key
            ),
        )
        .expect("failed to write ticket map");
        std::fs::write(
            &preparation,
            format!(
                "{{\"ticket_id\":\"prep-runtime\",\"message_key\":\"{}\"}}\n",
                child.message_key
            ),
        )
        .expect("failed to write prep tickets");
        std::fs::write(
            &run_queue,
            format!(
                "{{\"queue_key\":\"{}\",\"ticket_id\":\"prep-runtime\",\"status\":\"pending\"}}\n",
                child.message_key
            ),
        )
        .expect("failed to write run queue");
        let job = QueuedPrompt {
            prompt,
            goal: "Terminal-Bench 2 controller".to_string(),
            preview: "Terminal-Bench 2 controller".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: Some("benchmark-controller".to_string()),
            leased_message_keys: vec![leased.message_key],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/tb2-real-refs".to_string()),
            workspace_root: Some(run_dir.to_string_lossy().into_owned()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let expected = expected_outcome_artifacts_for_job(&job);
        let delivered = delivered_outcome_artifacts_for_job(&root, &job, &expected)
            .expect("failed to read delivered artifacts");

        assert_eq!(delivered.len(), 4);
        let proof_id = enforce_job_outcome_witness(&root, &job, expected, delivered)
            .expect("real queue runtime refs should satisfy Terminal-Bench witness")
            .expect("proof id should be returned");
        assert!(proof_id.starts_with("ctp-"));
    }

    #[test]
    fn workspace_file_recovery_prompt_names_missing_paths() {
        let job = QueuedPrompt {
            prompt: "RUN_DIR=\"/tmp/ctox-tb2-run\"\nInitialisiere die Dateien logbook.md und controller.json.".to_string(),
            goal: "bootstrap artifacts".to_string(),
            preview: "bootstrap artifacts".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-bootstrap".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let prompt =
            outcome_witness_recovery_message(Path::new(""), &job, "done", "missing artifact");

        assert!(prompt.contains("Datei-Artefakte fehlen"));
        assert!(prompt.contains("test -f"));
        assert!(prompt.contains("/tmp/ctox-tb2-run/logbook.md"));
        assert!(prompt.contains("/tmp/ctox-tb2-run/controller.json"));
        assert!(!prompt.contains("reviewed-founder-send"));
    }

    #[test]
    fn workspace_file_recovery_prompt_reports_directory_paths() {
        let root = temp_root("workspace-file-recovery-directory-path");
        let run_dir = root.join("tb2-run");
        let controller_path = run_dir.join("controller.json");
        std::fs::create_dir_all(&controller_path)
            .expect("failed to create directory at artifact path");
        let job = QueuedPrompt {
            prompt: format!(
                "Only required durable files for this controller turn:\n- {}\n",
                controller_path.display()
            ),
            goal: "bootstrap artifacts".to_string(),
            preview: "bootstrap artifacts".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-bootstrap".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let prompt = outcome_witness_recovery_message(&root, &job, "done", "missing artifact");

        assert!(prompt.contains(controller_path.to_string_lossy().as_ref()));
        assert!(prompt.contains("exists as a directory"));
        assert!(prompt.contains("test -d"));
        assert!(prompt.contains("regular file"));
    }

    #[test]
    fn workspace_file_recovery_prompt_reports_stale_fresh_artifacts() {
        let root = temp_root("workspace-file-recovery-stale-fresh-artifacts");
        let run_dir = root.join("tb2-run");
        let controller = run_dir.join("controller.json");
        let logbook = run_dir.join("logbook.md");
        std::fs::create_dir_all(&run_dir).expect("failed to create run dir");
        std::fs::write(&controller, "{}\n").expect("failed to write stale controller");
        std::fs::write(&logbook, "# old\n").expect("failed to write stale logbook");
        std::thread::sleep(Duration::from_secs(2));
        let prompt = format!(
            "CHECKPOINT-ONLY TERMINAL-BENCH RECOVERY SLICE\n\
Required output files to update now:\n\
- {}\n\
- {}\n\
Exit after the write command.",
            controller.display(),
            logbook.display()
        );
        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "checkpoint stale recovery".to_string(),
                prompt: prompt.clone(),
                thread_key: "queue/checkpoint-stale-recovery".to_string(),
                workspace_root: Some(run_dir.to_string_lossy().into_owned()),
                priority: "urgent".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create queue task");
        let leased = channels::lease_queue_task(&root, &task.message_key, "test")
            .expect("failed to lease queue task");
        let job = QueuedPrompt {
            prompt,
            goal: "checkpoint stale recovery".to_string(),
            preview: "checkpoint stale recovery".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec![leased.message_key],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("queue/checkpoint-stale-recovery".to_string()),
            workspace_root: Some(run_dir.to_string_lossy().into_owned()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let recovery = outcome_witness_recovery_message(&root, &job, "done", "missing artifact");

        assert!(recovery.contains("stale: regular file exists"));
        assert!(recovery.contains("not updated after the current queue lease"));
        assert!(recovery.contains("stale markierten Dateien in diesem Turn aktualisiert"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recovery_artifact_section_does_not_infer_paths_from_original_task() {
        let run_dir = "/tmp/ctox-tb2-run";
        let job = QueuedPrompt {
            prompt: format!(
                "HARNESS FEEDBACK\n\
Problem: expected artifacts are missing.\n\n\
REQUIRED ARTIFACTS\n\
These paths must exist as files:\n\
- {run_dir}/controller.json [missing]\n\
- {run_dir}/ticket-map.jsonl [missing]\n\
- {run_dir}/run-log.md [missing]\n\
- {run_dir}/results.jsonl [missing]\n\
- {run_dir}/summary.md [missing]\n\n\
NEXT ACTION\n\
Create the listed files.\n\n\
ORIGINAL TASK\n\
Mandatory checks mention /home/metricspace/ctox/runtime/ctox.sqlite3, \
/home/metricspace/ctox/runtime/inference_runtime.json, \
and labels like Qwen3.6-35B-A3B/controller.json or GPU/controller.json. \
Those are not durable artifact requirements."
            ),
            goal: "recover artifacts".to_string(),
            preview: "recover artifacts".to_string(),
            source_label: "queue".to_string(),
            suggested_skill: None,
            leased_message_keys: vec!["queue:tb2-controller".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: None,
            workspace_root: Some("/tmp".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: None,
        };

        let refs = expected_outcome_artifacts_for_job(&job);
        let paths = refs
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::WorkspaceFile)
            .map(|artifact| artifact.primary_key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                "/tmp/ctox-tb2-run/controller.json",
                "/tmp/ctox-tb2-run/ticket-map.jsonl",
                "/tmp/ctox-tb2-run/run-log.md",
                "/tmp/ctox-tb2-run/results.jsonl",
                "/tmp/ctox-tb2-run/summary.md",
            ]
        );
    }

    #[test]
    fn outcome_witness_accepts_delivered_mail_artifact() {
        let root = temp_root("outcome-witness-accepted-delivery");
        let conn = channels::open_channel_db(&crate::paths::core_db(&root))
            .expect("failed to open channel db");
        conn.execute(
            r#"
            INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id,
                direction, folder_hint, sender_display, sender_address,
                recipient_addresses_json, cc_addresses_json, bcc_addresses_json,
                subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at,
                observed_at, metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto@example.test', 'thread:julia', 'remote-1',
                'outbound', 'outbox', '', 'cto@example.test',
                '[]', '[]', '[]',
                'Subject', 'Preview', 'Body', '', '',
                'high', 'accepted', 1, 0, '2026-05-04T18:00:00Z',
                '2026-05-04T18:00:00Z', '{}'
            )
            "#,
            params!["email:cto@example.test::pending_send::abc"],
        )
        .expect("failed to insert accepted outbound row");
        drop(conn);
        let job = QueuedPrompt {
            prompt: "Schreibe den finalen Body.".to_string(),
            goal: "send mail".to_string(),
            preview: "send mail".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("thread:julia".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: Some(channels::FounderOutboundAction {
                account_key: "email:cto@example.test".to_string(),
                thread_key: "thread:julia".to_string(),
                subject: "Subject".to_string(),
                to: vec!["j.kienzler@example.test".to_string()],
                cc: Vec::new(),
                attachments: Vec::new(),
            }),
            outbound_anchor: Some("tui-outbound:test".to_string()),
        };
        let delivered = vec![ArtifactRef {
            kind: ArtifactKind::OutboundEmail,
            primary_key: "email:cto@example.test::pending_send::abc".to_string(),
            expected_terminal_state: "accepted".to_string(),
        }];

        let proof_id = enforce_job_outcome_witness(
            &root,
            &job,
            expected_outcome_artifacts_for_job(&job),
            delivered,
        )
        .expect("accepted outbound artifact should satisfy outcome witness")
        .expect("proof id should be returned");

        assert!(proof_id.starts_with("ctp-"));
    }

    #[test]
    fn outbound_recovery_prompt_gives_agent_exact_reviewed_send_step() {
        let approved_body =
            "Hallo Julia,\n\ndas ist der freigegebene Text.\n\nViele Gruesse\nINF Yoda";
        let job = QueuedPrompt {
            prompt: "Schreibe eine Mail an Julia.".to_string(),
            goal: "send mail".to_string(),
            preview: "send mail".to_string(),
            source_label: "tui".to_string(),
            suggested_skill: None,
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("julia-meeting-notetaker-report-20260505".to_string()),
            workspace_root: None,
            ticket_self_work_id: None,
            outbound_email: Some(channels::FounderOutboundAction {
                account_key: "email:INF.Yoda@remcapital.de".to_string(),
                thread_key: "julia-meeting-notetaker-report-20260505".to_string(),
                subject: "Erste Meeting-Teilnahme als INF Yoda Notetaker".to_string(),
                to: vec!["j.kienzler@remcapital.de".to_string()],
                cc: Vec::new(),
                attachments: Vec::new(),
            }),
            outbound_anchor: Some("tui-outbound:test".to_string()),
        };

        let prompt = outcome_witness_recovery_message(
            Path::new(""),
            &job,
            approved_body,
            "missing artifact",
        );

        assert!(prompt.contains("Die Review-Freigabe"));
        assert!(prompt.contains("Fuehre keine DB- oder Code-Forensik aus"));
        assert!(prompt.contains("BODY=$(cat <<'CTOX_REVIEWED_BODY'"));
        assert!(prompt.contains(approved_body));
        assert!(prompt.contains("ctox channel send --channel email"));
        assert!(prompt.contains("--reviewed-founder-send --body \"$BODY\""));
        assert!(!prompt.contains("<freigegebener Mailtext>"));
    }

    // F3: classify_agent_failure must produce stable, structured outcomes.
    #[test]
    fn classify_agent_failure_recognises_turn_timeout() {
        assert_eq!(
            classify_agent_failure("direct session timeout after 600s"),
            crate::lcm::AgentOutcome::TurnTimeout
        );
        assert_eq!(
            classify_agent_failure("turn timed out after 180s"),
            crate::lcm::AgentOutcome::TurnTimeout
        );
        assert_eq!(
            classify_agent_failure("hit the time budget"),
            crate::lcm::AgentOutcome::TurnTimeout
        );
    }

    #[test]
    fn classify_agent_failure_recognises_aborted_and_cancelled() {
        assert_eq!(
            classify_agent_failure("operator cancelled"),
            crate::lcm::AgentOutcome::Cancelled
        );
        assert_eq!(
            classify_agent_failure("invariant violated, aborted"),
            crate::lcm::AgentOutcome::Aborted
        );
        assert_eq!(
            classify_agent_failure("mid-task compaction timeout after 120s"),
            crate::lcm::AgentOutcome::Aborted
        );
    }

    #[test]
    fn classify_agent_failure_falls_back_to_execution_error() {
        assert_eq!(
            classify_agent_failure("connection refused"),
            crate::lcm::AgentOutcome::ExecutionError
        );
    }

    // F2: lcm helpers manage the per-mission failure counter and deferral.
    #[test]
    fn mission_failure_counter_increments_resets_and_defers() {
        let root = temp_root("mission-failure-counter");
        let db_path = root.join("ctox.sqlite3");
        let engine = LcmEngine::open(&db_path, LcmConfig::default()).unwrap();
        let _ = engine.continuity_init_documents(101).unwrap();
        let initial = engine.sync_mission_state_from_continuity(101).unwrap();
        assert_eq!(initial.agent_failure_count, 0);
        assert!(initial.deferred_reason.is_none());

        let after_one = engine.increment_mission_agent_failure_count(101).unwrap();
        assert_eq!(after_one.agent_failure_count, 1);
        let after_two = engine.increment_mission_agent_failure_count(101).unwrap();
        assert_eq!(after_two.agent_failure_count, 2);

        let after_reset = engine.reset_mission_agent_failure_count(101).unwrap();
        assert_eq!(after_reset.agent_failure_count, 0);

        let _ = engine.increment_mission_agent_failure_count(101).unwrap();
        let _ = engine.increment_mission_agent_failure_count(101).unwrap();
        let _ = engine.increment_mission_agent_failure_count(101).unwrap();
        let deferred = engine
            .defer_mission_for_reason(101, "agent_failure_threshold")
            .unwrap();
        assert_eq!(deferred.mission_status, "deferred");
        assert_eq!(
            deferred.deferred_reason.as_deref(),
            Some("agent_failure_threshold")
        );
        assert!(!deferred.is_open);
        assert!(deferred.allow_idle);
    }

    fn rewrite_finding(id: &str) -> review::CategorizedFinding {
        review::CategorizedFinding {
            id: id.to_string(),
            category: review::FindingCategory::Rewrite,
            evidence: format!("evidence for {id}"),
            corrective_action: format!("fix wording for {id}"),
        }
    }

    fn rework_finding(id: &str) -> review::CategorizedFinding {
        review::CategorizedFinding {
            id: id.to_string(),
            category: review::FindingCategory::Rework,
            evidence: format!("durable mismatch for {id}"),
            corrective_action: format!("create durable backing for {id}"),
        }
    }

    fn stale_finding(id: &str) -> review::CategorizedFinding {
        review::CategorizedFinding {
            id: id.to_string(),
            category: review::FindingCategory::StaleRefresh,
            evidence: format!("new inbound changed the thread for {id}"),
            corrective_action: format!("refresh current world state for {id}"),
        }
    }

    fn parent_outbound_job() -> QueuedPrompt {
        QueuedPrompt {
            prompt: "draft founder reply".to_string(),
            goal: "founder mail".to_string(),
            preview: "founder thread".to_string(),
            source_label: "email:owner".to_string(),
            suggested_skill: Some("communication-orchestrator".to_string()),
            leased_message_keys: vec!["email:cto1@example.com:msg-1".to_string()],
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("email-review:owner:thread-1".to_string()),
            workspace_root: Some("/srv/kunstmen".to_string()),
            ticket_self_work_id: None,
            outbound_email: None,
            outbound_anchor: Some("email:cto1@example.com:msg-1".to_string()),
        }
    }

    fn self_work_job() -> QueuedPrompt {
        QueuedPrompt {
            prompt: "work on CRM".to_string(),
            goal: "finish CRM integration".to_string(),
            preview: "CRM integration".to_string(),
            source_label: "ticket:self-work".to_string(),
            suggested_skill: Some("software-from-scratch".to_string()),
            leased_message_keys: Vec::new(),
            leased_ticket_event_keys: Vec::new(),
            thread_key: Some("ticket:crm".to_string()),
            workspace_root: Some("/srv/kunstmen".to_string()),
            ticket_self_work_id: Some("self-work:local:crm".to_string()),
            outbound_email: None,
            outbound_anchor: None,
        }
    }

    #[test]
    fn self_work_can_continue_before_completion_review_when_agent_requests_it() {
        let job = self_work_job();
        let work_id = continuation_self_work_requested(
            &job,
            "Ich habe den ersten Teil umgesetzt. Noch offen: Browser-QA und DB-Smoke-Test. Mach weiter mit dem naechsten konkreten Schritt.",
        );
        assert_eq!(work_id, Some("self-work:local:crm"));
    }

    #[test]
    fn founder_mail_never_uses_continue_shortcut_before_review() {
        let mut job = parent_outbound_job();
        job.ticket_self_work_id = Some("self-work:local:mail".to_string());
        let work_id = continuation_self_work_requested(
            &job,
            "Noch offen: die Antwort sauber formulieren. Mach weiter.",
        );
        assert_eq!(work_id, None);
    }

    #[test]
    fn dispatcher_routes_all_rewrite_findings_to_rewrite_only() {
        let findings = vec![rewrite_finding("f1"), rewrite_finding("f2")];
        assert_eq!(
            classify_findings(&findings),
            ReviewRoutingClass::RewriteOnly
        );
    }

    #[test]
    fn dispatcher_routes_mixed_findings_to_requeue_self_work() {
        let mixed = vec![rewrite_finding("f1"), rework_finding("f2")];
        assert_eq!(classify_findings(&mixed), ReviewRoutingClass::Substantive);
        let only_rework = vec![rework_finding("f1")];
        assert_eq!(
            classify_findings(&only_rework),
            ReviewRoutingClass::Substantive
        );
    }

    #[test]
    fn dispatcher_routes_stale_findings_to_stale_refresh() {
        let only_stale = vec![stale_finding("f1")];
        assert_eq!(classify_findings(&only_stale), ReviewRoutingClass::Stale);
        let mixed_stale = vec![rewrite_finding("f1"), stale_finding("f2")];
        assert_eq!(classify_findings(&mixed_stale), ReviewRoutingClass::Stale);
    }

    #[test]
    fn review_failure_for_self_work_requeues_same_main_work_without_cascade() {
        let root = temp_root("ctox-review-no-cascade");
        let item = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "mission-follow-up".to_string(),
                title: "Finish durable mission work".to_string(),
                body_text: "Do the durable work once.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": "ticket:no-cascade",
                    "priority": "high",
                }),
            },
            false,
        )
        .expect("failed to create self-work");
        let mut job = self_work_job();
        job.ticket_self_work_id = Some(item.work_id.clone());
        let mut outcome = review::ReviewOutcome::skipped("still insufficient");
        outcome.verdict = review::ReviewVerdict::Fail;
        outcome.required = true;

        let disposition = no_cascade_review_block(&root, &job, &outcome)
            .expect("self-work review failure should be fed back into main work");

        assert!(matches!(
            disposition,
            CompletionReviewDisposition::RequeueSelfWork { .. }
        ));

        let conn = channels::open_channel_db(&crate::paths::core_db(&root))
            .expect("failed to open channel db");
        let accepted_count: i64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM ctox_core_transition_proofs
                WHERE entity_type = 'WorkItem'
                  AND entity_id = ?1
                  AND from_state = 'AwaitingReview'
                  AND to_state = 'ReworkRequired'
                  AND accepted = 1
                "#,
                params![item.work_id],
                |row| row.get(0),
            )
            .expect("failed to count checkpoint proofs");
        assert_eq!(accepted_count, 1);
    }

    #[test]
    fn dispatcher_routes_empty_findings_to_approved() {
        let empty: Vec<review::CategorizedFinding> = Vec::new();
        assert_eq!(classify_findings(&empty), ReviewRoutingClass::Approved);
    }

    #[test]
    fn rewrite_only_post_turn_spawns_lightweight_pending_prompt() {
        let _ = temp_root("rewrite-only-post-turn");
        let parent = parent_outbound_job();
        let findings = vec![
            RewriteFinding {
                id: "f1".to_string(),
                evidence: "salutation uses internal vocab".to_string(),
                corrective_action: "use neutral salutation".to_string(),
            },
            RewriteFinding {
                id: "f2".to_string(),
                evidence: "body too long".to_string(),
                corrective_action: "trim to two paragraphs".to_string(),
            },
        ];
        let prior_body = "Hallo TUI-Founder, hier kommt der Stand…".to_string();
        let synthesised = synthesise_review_rewrite_prompt(
            &parent,
            &findings,
            &prior_body,
            parent.outbound_anchor.as_deref(),
            "two wording issues to address",
        );

        assert_eq!(synthesised.source_label, REVIEW_REWRITE_SOURCE_LABEL);
        assert!(synthesised.leased_message_keys.is_empty());
        assert_eq!(
            synthesised.outbound_email.is_some(),
            parent.outbound_email.is_some()
        );
        assert_eq!(synthesised.outbound_anchor, parent.outbound_anchor);
        assert_eq!(synthesised.thread_key, parent.thread_key);
        assert!(synthesised.ticket_self_work_id.is_none());
        assert!(synthesised.prompt.contains(&prior_body));
        assert!(synthesised
            .prompt
            .contains("salutation uses internal vocab"));
        assert!(synthesised.prompt.contains("trim to two paragraphs"));
        assert!(synthesised.prompt.contains("nur der korrigierte Body"));

        let mut shared = SharedState::default();
        shared.pending_prompts.push_front(synthesised);
        assert_eq!(shared.pending_prompts.len(), 1);
        let front = shared.pending_prompts.front().unwrap();
        assert_eq!(front.source_label, REVIEW_REWRITE_SOURCE_LABEL);
        assert_eq!(front.outbound_anchor, parent.outbound_anchor);
        // No durable side effects: no ticket id and no plan goal/step row
        // could exist because we never called plan::ingest. Confirming the
        // synthesis path itself never inherited a ticket id is enough.
        assert!(front.ticket_self_work_id.is_none());
    }

    #[test]
    fn rewrite_failure_count_threshold_defers_mission() {
        let root = temp_root("rewrite-threshold-defer");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let db_path = crate::paths::core_db(&root);
        let engine = LcmEngine::open(&db_path, LcmConfig::default()).unwrap();
        // Seed an initial mission so the counter has somewhere to land.
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap();
        let _ = engine
            .sync_mission_state_from_continuity(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap();

        let threshold = mission_rewrite_failure_threshold();
        for _ in 0..threshold {
            let _ = engine
                .increment_mission_rewrite_failure_count(turn_loop::CHAT_CONVERSATION_ID)
                .unwrap();
        }
        let pre_defer = engine
            .stored_mission_state(turn_loop::CHAT_CONVERSATION_ID)
            .unwrap()
            .unwrap();
        assert_eq!(pre_defer.rewrite_failure_count, threshold);

        let deferred = engine
            .defer_mission_for_reason(turn_loop::CHAT_CONVERSATION_ID, "rewrite_failure_threshold")
            .unwrap();
        assert_eq!(deferred.mission_status, "deferred");
        assert_eq!(
            deferred.deferred_reason.as_deref(),
            Some("rewrite_failure_threshold")
        );
        assert!(!deferred.is_open);

        let _ = governance::record_event(
            &root,
            governance::GovernanceEventRequest {
                mechanism_id: "review_rewrite_threshold",
                conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
                severity: "warning",
                reason: "rewrite-only review iterations failed to converge",
                action_taken: "deferred mission and stopped respawning rewrite retries",
                details: serde_json::json!({"threshold": threshold}),
                idempotence_key: Some("rewrite-threshold-test"),
            },
        );
        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events
            .iter()
            .any(|event| event.mechanism_id == "review_rewrite_threshold"));
    }

    /// Bug #1: an inbound founder mail flagged via the structured
    /// RFC 3834 Auto-Submitted marker must NOT be promoted into
    /// `review_rework`. The repair pass should record a structured
    /// NO-SEND verdict and leave the routing state alone.
    #[test]
    fn auto_submitted_founder_mail_is_not_classified_as_founder_reply() {
        let root = temp_root("ctox-auto-submitted-no-rework");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "j.cakmak@remcapital.de".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &settings)
            .expect("failed to persist owner setting");

        let inbound_key = "email:cto1@metric-space.ai::INBOX::ooo-jill";
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        // Subject in German, identical to a real human reply, on
        // purpose: only the structured Auto-Submitted metadata field
        // should determine classification. No string scraping.
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', 'thread-ooo-1',
                'remote-ooo-1', 'inbound', 'INBOX', 'Jill Cakmak',
                'j.cakmak@remcapital.de', '[]', '[]', '[]',
                'Re: REM Capital Förderanträge',
                'Bin im Urlaub.',
                'Bin im Urlaub bis 2026-05-12.',
                '', '', 'high', 'received', 0, 0,
                '2026-04-27T09:00:00Z', '2026-04-27T09:00:00Z',
                '{"autoSubmitted": true, "autoSubmittedValue": "auto-replied"}'
            )"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert auto-submitted founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'failed', NULL, NULL, NULL, NULL, '2026-04-27T09:00:00Z')"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert routing state");
        drop(conn);

        let state = Arc::new(Mutex::new(SharedState::default()));
        let _ = repair_stalled_founder_communications(&root, &state, &settings)
            .expect("repair pass should run");

        // No founder-communication rework queue task spawned.
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        let founder_rework_count = tasks
            .iter()
            .filter(|task| task.title.starts_with("Founder communication rework:"))
            .count();
        assert_eq!(
            founder_rework_count, 0,
            "auto-submitted inbound must not spawn founder rework"
        );
        // A structured NO-SEND verdict must have been persisted.
        assert!(
            channels::inbound_message_has_terminal_no_send(&root, inbound_key)
                .expect("verdict lookup")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Bug #2: once an inbound founder mail (one that does not require
    /// a reviewed reply, e.g. auto-submitted) is acked into `handled`,
    /// further iterations of the repair loop must NOT pull it back into
    /// `review_rework`.
    #[test]
    fn handled_route_status_is_sticky_for_auto_submitted_inbound() {
        let root = temp_root("ctox-handled-sticky");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "d.lottes@remcapital.de".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &settings)
            .expect("failed to persist owner setting");

        let inbound_key = "email:cto1@metric-space.ai::INBOX::ooo-dom";
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', 'thread-ooo-2',
                'remote-ooo-2', 'inbound', 'INBOX', 'Dominic Lottes',
                'd.lottes@remcapital.de', '[]', '[]', '[]',
                'Re: REM Capital Förderanträge', 'Out of office.', 'Bin out of office bis 2026-05-12.',
                '', '', 'high', 'received', 0, 0,
                '2026-04-27T10:00:00Z', '2026-04-27T10:00:00Z',
                '{"autoSubmitted": true, "autoSubmittedValue": "auto-replied"}'
            )"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert founder inbound");
        // Pre-existing handled state with no acked_at (the operator
        // ack path leaves a `handled` row that's missing a reviewed
        // reply).
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'handled', NULL, NULL, '2026-04-27T11:00:00Z', NULL, '2026-04-27T11:00:00Z')"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert handled state");
        drop(conn);

        let state = Arc::new(Mutex::new(SharedState::default()));
        // Run the repair pass twice — the second iteration must be a
        // no-op for this inbound (Bug #2 was: each iteration flipped
        // back into review_rework).
        for _ in 0..2 {
            let _ = repair_stalled_founder_communications(&root, &state, &settings)
                .expect("repair pass should run");
        }

        let conn = channels::open_channel_db(&db_path).expect("reopen channel db");
        let route_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                rusqlite::params![inbound_key],
                |row| row.get(0),
            )
            .expect("failed to load route status");
        assert_eq!(
            route_status, "handled",
            "handled state must be sticky for auto-submitted founder inbound"
        );
        // No rework queue task created.
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert!(tasks
            .iter()
            .all(|task| !task.title.starts_with("Founder communication rework:")));
        let _ = std::fs::remove_dir_all(root);
    }

    /// Bug #3: once a NO-SEND verdict has been recorded for an inbound
    /// message_key, no later pass may spawn a founder-communication
    /// rework that would functionally overwrite that verdict.
    #[test]
    fn rework_spawn_is_blocked_when_terminal_no_send_verdict_exists() {
        let root = temp_root("ctox-no-send-blocks-rework");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::no-send-keep";
        // First, persist the inbound message and a NO-SEND verdict.
        let db_path = crate::paths::core_db(&root);
        let conn = channels::open_channel_db(&db_path).expect("failed to open channel db");
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', 'thread-no-send',
                'remote-no-send', 'inbound', 'INBOX', 'Jill Cakmak',
                'j.cakmak@remcapital.de', '[]', '[]', '[]',
                'Re: Förderanträge', 'preview', 'irrelevant body text',
                '', '', 'high', 'received', 0, 0,
                '2026-04-27T09:00:00Z', '2026-04-27T09:00:00Z',
                '{"autoSubmitted": true}'
            )"#,
            rusqlite::params![inbound_key],
        )
        .expect("failed to insert founder inbound");
        drop(conn);
        channels::record_terminal_no_send_verdict(
            &root,
            inbound_key,
            "external-review",
            "Jill's April 27, 2026 message is only an out-of-office auto-reply, so this thread should remain unanswered until there is a substantive founder reply.",
        )
        .expect("failed to record NO-SEND verdict");

        let routed = channels::RoutedInboundMessage {
            message_key: inbound_key.to_string(),
            channel: "email".to_string(),
            account_key: "email:cto1@metric-space.ai".to_string(),
            thread_key: "thread-no-send".to_string(),
            sender_display: "Jill Cakmak".to_string(),
            sender_address: "j.cakmak@remcapital.de".to_string(),
            subject: "Re: Förderanträge".to_string(),
            preview: "preview".to_string(),
            body_text: "irrelevant body text".to_string(),
            external_created_at: "2026-04-27T09:00:00Z".to_string(),
            workspace_root: None,
            metadata: serde_json::json!({}),
            preferred_reply_modality: None,
        };
        let changed = ensure_founder_communication_rework_runnable(
            &root,
            &routed,
            "Founder communication is stalled without a reviewed sent reply; restore the existing rework",
        )
        .expect("rework runnable check should not error");
        assert!(
            !changed,
            "NO-SEND verdict must veto the rework spawn (Bug #3)"
        );

        // No queue task created.
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert!(
            tasks
                .iter()
                .all(|task| !task.title.starts_with("Founder communication rework:")),
            "no founder rework should have been enqueued"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
