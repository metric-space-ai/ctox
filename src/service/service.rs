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
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::collections::VecDeque;
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
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Once;
use std::thread;
use std::time::Duration;
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
use crate::context_health;
use crate::governance;
use crate::inference::runtime_control;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::supervisor;
use crate::inference::turn_loop;
use crate::lcm;
use crate::mission::communication_adapters;
use crate::mission::tickets;
use crate::schedule;
use crate::scrape;
use crate::state_invariants;

#[cfg(not(unix))]
const DEFAULT_SERVICE_HOST: &str = "127.0.0.1";
#[cfg(not(unix))]
const DEFAULT_SERVICE_PORT: &str = "12435";
const SERVICE_PID_RELATIVE_PATH: &str = "runtime/ctox_service.pid";
const SERVICE_LOG_RELATIVE_PATH: &str = "runtime/ctox_service.log";
const SERVICE_SOCKET_RELATIVE_PATH: &str = "runtime/ctox_service.sock";
const SYSTEMD_USER_UNIT_NAME: &str = "ctox.service";
const CHANNEL_ROUTER_POLL_SECS: u64 = 8;
const MISSION_WATCHER_POLL_SECS: u64 = 15;
const CHANNEL_ROUTER_LEASE_OWNER: &str = "ctox-service";
const QUEUE_PRESSURE_GUARD_THRESHOLD: usize = 6;
const QUEUE_GUARD_SOURCE_LABEL: &str = "queue-guard";
const SERVICE_SHUTDOWN_TIMEOUT_SECS: u64 = 15;
const SERVICE_SHUTDOWN_POLL_MILLIS: u64 = 150;
const SYSTEMCTL_USER_TIMEOUT_SECS: u64 = 5;

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
    })
}

#[cfg(not(unix))]
#[derive(Debug, Serialize, Deserialize)]
struct ChatSubmitRequest {
    prompt: String,
    #[serde(default)]
    thread_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AcceptedResponse {
    accepted: bool,
    status: String,
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
    channels::ensure_store(root)?;
    governance::ensure_governance(root)?;
    let db_path = root.join("runtime/ctox_lcm.db");
    let _ = crate::lcm::LcmEngine::open(&db_path, crate::lcm::LcmConfig::default())?;
    let listen_addr = service_listen_addr(root);
    write_pid_file(root, std::process::id())?;
    let state = Arc::new(Mutex::new(SharedState::default()));
    run_boot_state_invariant_check(root, &state);
    push_event(&state, format!("Loop ready on {}", listen_addr));
    start_channel_router(root.to_path_buf(), state.clone());
    start_channel_syncer(root.to_path_buf());
    start_mission_watcher(root.to_path_buf(), state.clone());
    // The service control plane must come up independently of backend warmup.
    supervisor::start_backend_supervisor(root.to_path_buf());
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
    let db_path = root.join("runtime/ctox_lcm.db");
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
        for _ in 0..40 {
            thread::sleep(Duration::from_millis(150));
            let status = service_status_snapshot(root)?;
            if status.running {
                return Ok(format!(
                    "CTOX service enabled and started via systemd on {}",
                    status.listen_addr
                ));
            }
        }
        return Ok("CTOX systemd service start requested.".to_string());
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
    #[cfg(unix)]
    {
        match send_service_ipc_request(
            root,
            ServiceIpcRequest::ChatSubmit {
                prompt: prompt.to_string(),
                thread_key: None,
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
            prompt: prompt.to_string(),
            thread_key: None,
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
        ServiceIpcRequest::ChatSubmit { prompt, thread_key } => {
            let workspace_root = channels::legacy_workspace_root_from_prompt(&prompt);
            let queued = {
                let mut shared = lock_shared_state(&state);
                if shared.busy || runtime_blocker_backoff_remaining_secs(&shared).is_some() {
                    shared.pending_prompts.push_back(QueuedPrompt {
                        preview: preview_text(&prompt),
                        source_label: "tui".to_string(),
                        goal: prompt.clone(),
                        prompt: prompt.clone(),
                        suggested_skill: None,
                        leased_message_keys: Vec::new(),
                        leased_ticket_event_keys: Vec::new(),
                        thread_key: thread_key.clone(),
                        workspace_root: workspace_root.clone(),
                    });
                    ensure_queue_guard_locked(root, &mut shared);
                    let pending = shared.pending_prompts.len();
                    let reason = runtime_blocker_backoff_remaining_secs(&shared)
                        .map(|secs| format!("runtime blocker cooldown {secs}s"))
                        .unwrap_or_else(|| "service busy".to_string());
                    push_event_locked(
                        &mut shared,
                        format!("Queued follow-up prompt #{pending} ({reason})"),
                    );
                    true
                } else {
                    shared.busy = true;
                    shared.current_goal_preview = Some(preview_text(&prompt));
                    shared.active_source_label = Some("tui".to_string());
                    shared.last_error = None;
                    shared.last_reply_chars = None;
                    push_event_locked(&mut shared, "Started prompt".to_string());
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
                        suggested_skill: None,
                        leased_message_keys: Vec::new(),
                        leased_ticket_event_keys: Vec::new(),
                        thread_key,
                        workspace_root,
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
            let workspace_root = channels::legacy_workspace_root_from_prompt(&payload.prompt);
            let queued = {
                let mut shared = lock_shared_state(&state);
                if shared.busy || runtime_blocker_backoff_remaining_secs(&shared).is_some() {
                    shared.pending_prompts.push_back(QueuedPrompt {
                        preview: preview_text(&payload.prompt),
                        source_label: "tui".to_string(),
                        goal: payload.prompt.clone(),
                        prompt: payload.prompt.clone(),
                        suggested_skill: None,
                        leased_message_keys: Vec::new(),
                        leased_ticket_event_keys: Vec::new(),
                        thread_key: payload.thread_key.clone(),
                        workspace_root: workspace_root.clone(),
                    });
                    ensure_queue_guard_locked(root, &mut shared);
                    let pending = shared.pending_prompts.len();
                    let reason = runtime_blocker_backoff_remaining_secs(&shared)
                        .map(|secs| format!("runtime blocker cooldown {secs}s"))
                        .unwrap_or_else(|| "service busy".to_string());
                    push_event_locked(
                        &mut shared,
                        format!("Queued follow-up prompt #{pending} ({reason})"),
                    );
                    true
                } else {
                    shared.busy = true;
                    shared.current_goal_preview = Some(preview_text(&payload.prompt));
                    shared.active_source_label = Some("tui".to_string());
                    shared.last_error = None;
                    shared.last_reply_chars = None;
                    push_event_locked(&mut shared, "Started prompt".to_string());
                    false
                }
            };
            if !queued {
                start_prompt_worker(
                    root.to_path_buf(),
                    state.clone(),
                    QueuedPrompt {
                        preview: preview_text(&payload.prompt),
                        source_label: "tui".to_string(),
                        goal: payload.prompt.clone(),
                        prompt: payload.prompt,
                        suggested_skill: None,
                        leased_message_keys: Vec::new(),
                        leased_ticket_event_keys: Vec::new(),
                        thread_key: payload.thread_key,
                        workspace_root,
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

    let durable_tasks =
        channels::list_queue_tasks(root, &["pending".to_string(), "leased".to_string()], 6)
            .unwrap_or_default();
    let ticket_cases = tickets::list_cases(root, None, 6).unwrap_or_default();
    for task in &durable_tasks {
        if pending_previews.len() >= 6 {
            break;
        }
        let preview = format!("queue  {}", clip_text(task.title.trim(), 120));
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

fn service_ipc_timeout(request: &ServiceIpcRequest) -> Duration {
    match request {
        ServiceIpcRequest::Status => Duration::from_secs(5),
        ServiceIpcRequest::ScrapeApi { .. } => Duration::from_millis(750),
        ServiceIpcRequest::ChatSubmit { .. } | ServiceIpcRequest::Stop => {
            Duration::from_millis(300)
        }
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
    let candidate = root.join("target/release/ctox");
    if candidate.is_file() {
        return Ok(candidate);
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
    let candidate_display = root.join("target/release/ctox").display().to_string();
    if !displays.iter().any(|entry| entry == &candidate_display) {
        displays.push(candidate_display);
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
        eprintln!(
            "ctox prompt worker start source={} preview={}",
            job.source_label,
            clip_text(&job.preview, 120)
        );
        let panic_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db_path = root.join("runtime/ctox_lcm.db");
            let event_state = state.clone();
            let event_source = job.source_label.clone();
            let workspace_root = job.workspace_root.as_deref().map(std::path::Path::new);
            let conversation_id =
                turn_loop::conversation_id_for_thread_key(job.thread_key.as_deref());
            let result = turn_loop::run_chat_turn_with_events(
                &root,
                &db_path,
                &job.prompt,
                workspace_root,
                conversation_id,
                job.suggested_skill.as_deref(),
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
            let failure_reply = result.as_ref().err().map(|err| {
                if let Some(title) = timeout_follow_up_outcome.as_ref() {
                    format!(
                        "Status: `deferred`\n\nCheckpoint: the slice hit the turn time budget and a durable continuation task was queued: {title}\n\nLatest runtime summary: {}",
                        turn_loop::summarize_runtime_error(&err.to_string())
                    )
                } else {
                    turn_loop::synthesize_failure_reply(&err.to_string())
                }
            });
            if let Some(reply) = &failure_reply {
                let _ = lcm::run_add_message(&db_path, conversation_id, "assistant", reply);
            }
            let latest_runtime_error = result.as_ref().err().map(|err| err.to_string());
            let context_health =
                assess_current_context_health(&root, &db_path, conversation_id, Some(&job.prompt));
            let mut mission_sync_outcome =
                lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())
                    .and_then(|engine| engine.sync_mission_state_from_continuity(conversation_id))
                    .ok();
            if let Some(repaired) =
                run_turn_end_state_invariant_check(&root, &state, conversation_id)
            {
                mission_sync_outcome = Some(repaired);
            }
            let mut next_prompt = None;
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
                        if !job.leased_message_keys.is_empty() {
                            let _ = channels::ack_leased_messages(
                                &root,
                                &job.leased_message_keys,
                                "handled",
                            );
                        }
                        if !job.leased_ticket_event_keys.is_empty() {
                            let _ = tickets::ack_leased_ticket_events(
                                &root,
                                &job.leased_ticket_event_keys,
                                "handled",
                            );
                        }
                        shared.last_error = None;
                        shared.last_reply_chars = Some(reply.chars().count());
                        push_event_locked(
                            &mut shared,
                            format!(
                                "Completed {} reply with {} chars",
                                job.source_label,
                                reply.chars().count()
                            ),
                        );
                    }
                    Err(err) => {
                        let err_text = err.to_string();
                        let compact_error = turn_loop::summarize_runtime_error(&err_text);
                        if !job.leased_message_keys.is_empty() {
                            let _ = channels::ack_leased_messages(
                                &root,
                                &job.leased_message_keys,
                                "failed",
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
                        push_event_locked(
                            &mut shared,
                            format!("{} prompt failed: {compact_error}", job.source_label),
                        );
                        if let Some(title) = &timeout_follow_up_outcome {
                            push_event_locked(
                                &mut shared,
                                format!("Created timeout continuation task: {title}"),
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
                    next_prompt = maybe_start_next_queued_prompt_locked(&mut shared);
                }
            }
            if let Some(queued) = next_prompt {
                start_prompt_worker(root.clone(), state.clone(), queued);
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
                    next_prompt = maybe_start_next_queued_prompt_locked(&mut shared);
                }
            }
            if let Some(queued) = next_prompt {
                start_prompt_worker(root, state, queued);
            }
            eprintln!("ctox prompt worker end source={} panic", job.source_label);
        }
    });
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
        let settings = runtime_env::load_runtime_env_map(&root).unwrap_or_default();
        sync_configured_channels(&root, &settings);
        thread::sleep(Duration::from_secs(CHANNEL_ROUTER_POLL_SECS));
    });
}

fn start_mission_watcher(root: std::path::PathBuf, state: Arc<Mutex<SharedState>>) {
    thread::spawn(move || loop {
        if let Err(err) = monitor_mission_continuity(&root, &state) {
            push_event(&state, format!("Mission watcher failed: {err}"));
        }
        thread::sleep(Duration::from_secs(MISSION_WATCHER_POLL_SECS));
    });
}

fn monitor_mission_continuity(root: &Path, state: &Arc<Mutex<SharedState>>) -> Result<()> {
    if mission_watcher_disabled(root) {
        return Ok(());
    }
    let (last_progress_epoch_secs, last_error) = {
        let shared = lock_shared_state(state);
        if shared.busy || !shared.pending_prompts.is_empty() {
            return Ok(());
        }
        (shared.last_progress_epoch_secs, shared.last_error.clone())
    };
    if runnable_queue_work_exists(root)? {
        return Ok(());
    }

    let db_path = root.join("runtime/ctox_lcm.db");
    let engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;
    let mission = engine.sync_mission_state_from_continuity(turn_loop::CHAT_CONVERSATION_ID)?;
    if !mission.is_open || mission.allow_idle {
        return Ok(());
    }

    let idle_secs = current_epoch_secs().saturating_sub(last_progress_epoch_secs);
    if let Some(error) = last_error.as_deref() {
        if let Some(cooldown_secs) = turn_loop::hard_runtime_blocker_retry_cooldown_secs(error) {
            if idle_secs < cooldown_secs {
                return Ok(());
            }
        }
    }
    if idle_secs < mission_idle_tolerance_secs(&mission) {
        return Ok(());
    }

    let thread_key = mission_thread_key(mission.conversation_id);
    if runnable_thread_task_exists(root, &thread_key)? {
        return Ok(());
    }

    let title = if mission.mission.trim().is_empty() {
        format!("Continue mission {}", mission.conversation_id)
    } else {
        format!("Continue mission {}", clip_text(&mission.mission, 48))
    };
    let created = channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title,
            prompt: render_mission_continuation_prompt(&mission, idle_secs),
            thread_key,
            priority: mission_task_priority(&mission).to_string(),
            workspace_root: None,
            suggested_skill: Some("follow-up-orchestrator".to_string()),
            parent_message_key: None,
        },
    )?;
    let triggered_at = now_iso_string();
    let _ = engine.note_mission_watcher_triggered(mission.conversation_id, &triggered_at)?;
    let event_key = format!("mission-watchdog:{}", mission.conversation_id);
    let _ = governance::record_event(
        root,
        governance::GovernanceEventRequest {
            mechanism_id: "mission_idle_watchdog",
            conversation_id: Some(mission.conversation_id),
            severity: "warning",
            reason: "open mission stayed idle beyond the tolerated window",
            action_taken: "queued a mission continuation slice",
            details: serde_json::json!({
                "conversation_id": mission.conversation_id,
                "idle_secs": idle_secs,
                "thread_key": created.thread_key.clone(),
                "title": created.title.clone(),
            }),
            idempotence_key: Some(&event_key),
        },
    );
    push_event(
        state,
        format!(
            "Mission watcher re-triggered open mission after {}s idle: {}",
            idle_secs, created.title
        ),
    );
    Ok(())
}

fn mission_watcher_disabled(root: &Path) -> bool {
    let value = runtime_env::env_or_config(root, "CTOX_DISABLE_MISSION_WATCHDOG")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(value.as_str(), "1" | "true" | "yes" | "on")
}

fn route_external_messages(root: &Path, state: &Arc<Mutex<SharedState>>) -> Result<()> {
    if queue_pressure_active(state) {
        return Ok(());
    }
    route_assigned_ticket_self_work(root, state)?;
    let settings = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    let scheduled = schedule::emit_due_tasks(root)?;
    if scheduled.emitted_count > 0 {
        push_event(
            state,
            format!("Scheduled {} cron task(s)", scheduled.emitted_count),
        );
    }
    sync_configured_tickets(root, &settings);
    let leased = channels::lease_pending_inbound_messages(root, 16, CHANNEL_ROUTER_LEASE_OWNER)?;
    let mut seen = HashSet::new();
    let mut duplicates = Vec::new();
    let mut blocked = Vec::new();
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
        let prompt = enrich_inbound_prompt(root, &settings, &message, &prompt_body);
        let leased_message_key = message.message_key.clone();
        if inflight_leased_message_key(state, &leased_message_key) {
            continue;
        }
        enqueue_prompt(
            root,
            state,
            QueuedPrompt {
                preview: preview_text(&prompt),
                source_label: message.channel.clone(),
                goal: prompt_body.clone(),
                prompt,
                suggested_skill: suggested_skill_from_message(&message),
                leased_message_keys: vec![leased_message_key],
                leased_ticket_event_keys: Vec::new(),
                thread_key: Some(message.thread_key.clone()),
                workspace_root: message.workspace_root.clone(),
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
    route_ticket_events(root, state)?;
    Ok(())
}

fn route_assigned_ticket_self_work(root: &Path, state: &Arc<Mutex<SharedState>>) -> Result<()> {
    let items = tickets::list_ticket_self_work_items(root, None, Some("published"), 128)?;
    for item in items {
        if item.assigned_to.as_deref() != Some("self") {
            continue;
        }
        // system-onboarding self-work items are now routed normally
        // so the model can execute onboarding steps autonomously.
        let thread_key = format!("ticket-self-work:{}", item.work_id);
        if runnable_thread_task_exists(root, &thread_key)? {
            continue;
        }
        let mut prompt_lines = vec![
            format!(
                "Bearbeite das veroeffentlichte CTOX-Self-Work fuer {}.",
                item.source_system
            ),
            format!("Titel: {}", item.title.trim()),
            format!("Art: {}", item.kind.trim()),
            format!("Work-ID: {}", item.work_id.trim()),
            String::new(),
            item.body_text.trim().to_string(),
        ];
        if let Some(locator) = item
            .remote_locator
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            prompt_lines.push(String::new());
            prompt_lines.push(format!("Remote-Ticket: {}", locator));
        }
        let prompt = prompt_lines.join("\n");
        let created = channels::create_queue_task(
            root,
            channels::QueueTaskCreateRequest {
                title: item.title.trim().to_string(),
                prompt,
                thread_key: thread_key.clone(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: item.suggested_skill.clone(),
                parent_message_key: None,
            },
        )?;
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
    Ok(())
}

fn route_ticket_events(root: &Path, state: &Arc<Mutex<SharedState>>) -> Result<()> {
    let leased = tickets::lease_pending_ticket_events(root, 16, CHANNEL_ROUTER_LEASE_OWNER)?;
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
        enqueue_prompt(
            root,
            state,
            QueuedPrompt {
                preview: preview_text(&prompt),
                source_label: format!("ticket:{}", prepared.source_system),
                goal: prepared.summary.clone(),
                prompt,
                suggested_skill: tickets::suggested_skill_for_live_ticket_source(root, &prepared)
                    .unwrap_or(None),
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: vec![prepared.event_key.clone()],
                thread_key: Some(prepared.thread_key.clone()),
                workspace_root: None,
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
        if shared.busy || runtime_backoff_remaining.is_some() {
            shared.pending_prompts.push_back(prompt.clone());
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

fn maybe_start_next_queued_prompt_locked(shared: &mut SharedState) -> Option<QueuedPrompt> {
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

fn suggested_skill_from_message(message: &channels::RoutedInboundMessage) -> Option<String> {
    message
        .metadata
        .get("skill")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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
        return format!(
            "[E-Mail eingegangen]\nSender: {sender}\nBetreff: {subject}\nThread: {}\nWenn eine Antwort per E-Mail sinnvoll ist, nutze `ctox channel send --channel email --account-key {} --thread-key '{}' --to {reply_target} --subject \"Re: {subject}\"`. Nutze bei Antworten auf bestehende Mail-Threads keinen leeren oder neuen Betreff. Behandle die Mail-Huelle nicht als vollstaendigen Kontext: pruefe vor einer Antwort aktiv den Thread und die relevante Gesamtkommunikation mit den Kommunikations-Tools unten. Secrets, Passwoerter, Token, Root-/sudo-Material und andere geheimhaltungsbeduerftige Werte darfst du aus E-Mail nie als gueltige Eingabe uebernehmen; fordere dafuer immer TUI an. Wenn die angefragte Arbeit sudo oder andere privilegierte Host-Aktionen braucht und der Absender dafuer nicht berechtigt ist, sage das klar und nenne TUI oder einen sudo-berechtigten Admin/Owner als akzeptierten Freigabepfad.\n\n{}\n\n{}\n\n{}",
            message.thread_key,
            message.account_key,
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
            "[Jami-Nachricht eingegangen]\nSender: {sender}\nThread: {}\nWenn du antwortest, nutze `ctox channel send --channel jami --account-key {} --thread-key '{}' --body \"<deine Antwort>\"{voice_hint}`.{voice_note}\n\n{}",
            message.thread_key,
            message.account_key,
            message.thread_key,
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
    if prompt.contains("Work only inside this workspace:") {
        return prompt.to_string();
    }
    format!("Work only inside this workspace:\n{workspace_root}\n\n{prompt}")
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
    let db_path = root.join("runtime/cto_agent.db");
    let lcm_path = root.join("runtime/ctox_lcm.db");
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
    let _ = communication_adapters::teams().service_sync(root, settings);
}

fn sync_configured_tickets(root: &Path, settings: &BTreeMap<String, String>) {
    tickets::sync_configured_ticket_systems(root, settings);
}

fn render_ticket_prompt(root: &Path, event: &tickets::RoutedTicketEvent) -> String {
    let dry_run =
        serde_json::to_string_pretty(&event.dry_run_artifact).unwrap_or_else(|_| "{}".to_string());
    let ctox = preferred_ctox_executable(root)
        .unwrap_or_else(|_| root.join("target/release/ctox"))
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
        "token=",
        "secret:",
        "secret=",
        "api key:",
        "api-key:",
        "api_key=",
        "apikey=",
        "_password=",
        "_token=",
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
    let thread_key = job
        .thread_key
        .clone()
        .unwrap_or_else(|| default_follow_up_thread_key(&job.goal));
    let title = format!("Continue {} after timeout", clip_text(&job.goal, 48));
    let event_key = format!("timeout-continuation:{thread_key}:{title}");
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
                reason: "the previous turn hit the runtime time budget",
                action_taken: "reused an existing open continuation slice",
                details: serde_json::json!({
                    "source_label": job.source_label,
                    "thread_key": thread_key,
                    "title": title,
                    "existing_title": existing_title,
                    "blocker": clip_text(blocker, 180),
                }),
                idempotence_key: Some(&event_key),
            },
        );
        return Ok(Some(format!(
            "existing continuation reused: {existing_title}"
        )));
    }
    let created = channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title: title.clone(),
            prompt: render_timeout_continue_prompt(
                &job.goal,
                blocker,
                job.workspace_root.as_deref(),
            ),
            thread_key,
            workspace_root: job.workspace_root.clone(),
            priority: "high".to_string(),
            suggested_skill: job.suggested_skill.clone(),
            parent_message_key: job.leased_message_keys.first().cloned(),
        },
    )?;
    let _ = governance::record_event(
        root,
        governance::GovernanceEventRequest {
            mechanism_id: "turn_timeout_continuation",
            conversation_id: Some(turn_loop::CHAT_CONVERSATION_ID),
            severity: "warning",
            reason: "the previous turn hit the runtime time budget",
            action_taken: "queued a timeout continuation slice",
            details: serde_json::json!({
                "source_label": job.source_label,
                "thread_key": created.thread_key.clone(),
                "title": created.title.clone(),
                "blocker": clip_text(blocker, 180),
            }),
            idempotence_key: Some(&event_key),
        },
    );
    Ok(Some(created.title))
}

fn is_turn_timeout_blocker(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.contains("timed out after") || lowered.contains("time budget")
}

fn render_timeout_continue_prompt(
    goal: &str,
    blocker: &str,
    workspace_root: Option<&str>,
) -> String {
    let summarized_goal = summarize_follow_up_goal(goal);
    let prompt = format!(
        "Continue the interrupted task from the latest saved state.\n\nCurrent task:\n{}\n\nRuntime stop:\n{}\n\nRequired actions:\n- re-check repo, runtime, queue, progress artifacts, and continuity\n- preserve any work that already landed\n- continue with the next smallest concrete step\n- if more than one turn is still needed, leave exactly one open CTOX plan or queue item before the turn ends\n- a sentence in the reply does not count as open work\n- ask the owner only if the real blocker is external",
        summarized_goal,
        clip_text(blocker.trim(), 220)
    );
    prepend_workspace_contract(&prompt, workspace_root)
}

fn runnable_thread_task_exists(root: &Path, thread_key: &str) -> Result<bool> {
    let tasks =
        channels::list_queue_tasks(root, &["pending".to_string(), "leased".to_string()], 64)?;
    Ok(tasks.into_iter().any(|task| task.thread_key == thread_key))
}

fn runnable_queue_work_exists(root: &Path) -> Result<bool> {
    let tasks =
        channels::list_queue_tasks(root, &["pending".to_string(), "leased".to_string()], 64)?;
    Ok(!tasks.is_empty())
}

fn mission_thread_key(conversation_id: i64) -> String {
    format!("queue/mission-{conversation_id}")
}

fn mission_idle_tolerance_secs(mission: &lcm::MissionStateRecord) -> u64 {
    match normalize_token(&mission.trigger_intensity).as_str() {
        "hot" => 45,
        "warm" => 180,
        "cold" => 900,
        "archive" => 3_600,
        _ => match normalize_token(&mission.continuation_mode).as_str() {
            "continuous" => 45,
            "maintenance" => 180,
            "scheduled" => 900,
            "dormant" | "closed" => 3_600,
            _ => 120,
        },
    }
}

fn mission_task_priority(mission: &lcm::MissionStateRecord) -> &'static str {
    match normalize_token(&mission.trigger_intensity).as_str() {
        "hot" => "high",
        "warm" => "normal",
        "cold" | "archive" => "low",
        _ => "high",
    }
}

fn render_mission_continuation_prompt(mission: &lcm::MissionStateRecord, idle_secs: u64) -> String {
    let mission_label = if mission.mission.trim().is_empty() {
        "Keep the active mission alive from the latest durable continuity."
    } else {
        mission.mission.trim()
    };
    format!(
        "Mission continuity watchdog: the mission was idle for {idle_secs}s.\n\nMission: {mission_label}\nState: {mission_status}\nMode: {continuation_mode}\nIntensity: {trigger_intensity}\nBlocker: {blocker}\nNext step: {next_slice}\nTask is complete only when: {done_gate}\nClosure confidence: {closure_confidence}\n\nRequired actions:\n- re-check repo, runtime, queue, progress artifacts, and continuity\n- decide whether the mission is complete, safely handed off, or still open\n- if still open, do the next concrete step\n- if more than one turn remains, leave exactly one open CTOX plan or queue item\n- a sentence in the reply does not count as open work\n- do not let sidequests replace the mission\n- do not end in idle while the mission is still open",
        mission_status = clip_text(fallback_text(&mission.mission_status, "active"), 64),
        continuation_mode = clip_text(fallback_text(&mission.continuation_mode, "continuous"), 64),
        trigger_intensity = clip_text(fallback_text(&mission.trigger_intensity, "hot"), 64),
        blocker = clip_text(fallback_text(&mission.blocker, "none"), 180),
        next_slice = clip_text(
            fallback_text(
                &mission.next_slice,
                "reconstruct the next concrete slice from continuity",
            ),
            180,
        ),
        done_gate = clip_text(
            fallback_text(
                &mission.done_gate,
                "only close the mission when current evidence clearly satisfies the gate",
            ),
            180,
        ),
        closure_confidence = clip_text(fallback_text(&mission.closure_confidence, "low"), 64),
    )
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
    let ctox_bin =
        preferred_ctox_executable(root).unwrap_or_else(|_| root.join("target/release/ctox"));
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
        let db_path = root.join("runtime/ctox_lcm.db");
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
        let db_path = root.join("runtime/ctox_lcm.db");
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
        let db_path = root.join("runtime/ctox_lcm.db");
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
        let db_path = root.join("runtime/ctox_lcm.db");
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
        let db_path = root.join("runtime/ctox_lcm.db");
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
        let db_path = root.join("runtime/ctox_lcm.db");
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
        let db_path = root.join("runtime/ctox_lcm.db");
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
            },
        ]);

        ensure_queue_guard_locked(&root, &mut shared);

        assert!(shared
            .pending_prompts
            .iter()
            .all(|item| item.source_label != QUEUE_GUARD_SOURCE_LABEL));
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

        route_ticket_events(&root, &state).expect("ticket routing should succeed");

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

        route_ticket_events(&root, &state).expect("ticket routing should succeed");

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
        assert_eq!(
            prompt.suggested_skill.as_deref(),
            Some("system-onboarding")
        );
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
        });

        let next = maybe_start_next_queued_prompt_locked(&mut shared)
            .expect("queued prompt should be started");

        assert_eq!(
            next.suggested_skill.as_deref(),
            Some("system-onboarding")
        );
        assert!(shared.busy);
        assert_eq!(shared.active_source_label.as_deref(), Some("ticket:zammad"));
        assert!(shared.recent_events.iter().any(|event| event
            .contains("Started queued ticket:zammad prompt [skill system-onboarding]")));
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

        route_ticket_events(&root, &state).expect("ticket routing should succeed");

        let shared = state.lock().expect("state poisoned");
        assert_eq!(shared.pending_prompts.len(), 1);
        let prompt = shared
            .pending_prompts
            .front()
            .expect("queued prompt missing");
        assert_eq!(prompt.source_label, "ticket:local");
        assert_eq!(
            prompt.suggested_skill.as_deref(),
            Some("system-onboarding")
        );
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

        route_ticket_events(&root, &state).expect("ticket routing should succeed");

        let shared = state.lock().expect("state poisoned");
        assert_eq!(shared.pending_prompts.len(), 1);
        let prompt = shared
            .pending_prompts
            .front()
            .expect("queued prompt missing");
        assert_eq!(prompt.source_label, "ticket:local");
        assert_eq!(
            prompt.suggested_skill.as_deref(),
            Some("system-onboarding")
        );
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
        assert_eq!(
            prompt.suggested_skill.as_deref(),
            Some("system-onboarding")
        );
        let expected_thread_key = format!("ticket-self-work:{}", item.work_id);
        assert_eq!(
            prompt.thread_key.as_deref(),
            Some(expected_thread_key.as_str())
        );
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

        route_ticket_events(&root, &state).expect("ticket routing should succeed");

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

    #[test]
    fn timeout_continuation_prompt_summarizes_nested_goal() {
        let prompt = render_timeout_continue_prompt(
            "Continue the interrupted task from the latest durable state instead of treating it as externally blocked.\n\nGoal:\nMission continuity watchdog detected an open mission that went idle for 45 seconds.\n\nThe previous slice stopped because it hit the turn time budget:\ncodex-exec timed out after 900s",
            "codex-exec timed out after 900s",
            None,
        );
        assert!(
            prompt.contains("Slice goal:\nMission continuity watchdog detected an open mission")
        );
        assert!(prompt.contains("Runtime stop:\ncodex-exec timed out after 900s"));
        assert!(!prompt.contains("The previous slice stopped because it hit the turn time budget:\ncodex-exec timed out after 900s\n\nThe previous slice stopped"));
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
            "codex-exec timed out after 180s",
            Some("/tmp/ctox-workspace-contract"),
        );
        assert!(timeout_prompt
            .contains("Work only inside this workspace:\n/tmp/ctox-workspace-contract"));
        assert!(timeout_prompt.contains("Slice goal:\nShip the next implementation slice."));
    }

    #[test]
    fn mission_continuation_prompt_stays_compact() {
        let mission = lcm::MissionStateRecord {
            conversation_id: 1,
            mission: "Keep the active mission alive from the latest durable continuity."
                .to_string(),
            mission_status: "active".to_string(),
            continuation_mode: "continuous".to_string(),
            trigger_intensity: "hot".to_string(),
            blocker: "goal: reconstruct the next concrete slice from continuity".to_string(),
            next_slice: "inspect repo state and continue the smallest pending slice".to_string(),
            done_gate: "only close when current evidence satisfies the gate".to_string(),
            closure_confidence: "low".to_string(),
            is_open: true,
            allow_idle: false,
            focus_head_commit_id: "focus-1".to_string(),
            last_synced_at: "2026-04-06T00:00:00Z".to_string(),
            watcher_last_triggered_at: None,
            watcher_trigger_count: 0,
        };
        let prompt = render_mission_continuation_prompt(&mission, 45);
        assert!(prompt.contains("Mission continuity watchdog: the mission was idle for 45s."));
        assert!(prompt.contains("Required actions:"));
        assert!(prompt.len() < 900, "prompt too large: {}", prompt.len());
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
            Some("sender is outside the allowed email domain".to_string())
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

        let sudo_admin = channels::classify_email_sender(&settings, "opsadmin@example.com");
        assert_eq!(sudo_admin.role, "admin");
        assert!(sudo_admin.allow_admin_actions);
        assert!(sudo_admin.allow_sudo_actions);

        let plain_admin = channels::classify_email_sender(&settings, "helpdesk@example.com");
        assert_eq!(plain_admin.role, "admin");
        assert!(plain_admin.allow_admin_actions);
        assert!(!plain_admin.allow_sudo_actions);

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
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "codex-exec timed out after 180s")
                .expect("timeout continuation should succeed");

        assert_eq!(
            created.as_deref(),
            Some("existing continuation reused: spill restore: Deferred documentation review")
        );
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
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "codex-exec timed out after 180s")
                .expect("timeout continuation should succeed");

        assert_eq!(
            created.as_deref(),
            Some("existing continuation reused: spill restore: Restore monitoring follow-up")
        );
        let tasks =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)
                .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].message_key, existing.message_key);
    }

    #[test]
    fn timeout_blocker_queues_continuation_and_records_governance_event() {
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
        };

        let created =
            maybe_enqueue_timeout_continuation(&root, &job, "codex-exec timed out after 180s")
                .expect("timeout continuation should succeed");

        assert!(created.is_some());
        let tasks = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].thread_key, "tui/main");
        assert_eq!(
            tasks[0].suggested_skill.as_deref(),
            Some("change-lifecycle")
        );
        assert!(tasks[0].title.contains("after timeout"));
        assert!(tasks[0].prompt.contains("Continue the interrupted task"));
        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events
            .iter()
            .any(|event| event.mechanism_id == "turn_timeout_continuation"));
    }

    #[test]
    fn mission_watcher_enqueues_continuation_for_open_idle_mission() {
        let root = temp_root("ctox-mission-watcher-open");
        std::fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let engine =
            lcm::LcmEngine::open(&root.join("runtime/ctox_lcm.db"), lcm::LcmConfig::default())
                .expect("failed to open lcm");
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .expect("failed to init continuity");
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                lcm::ContinuityKind::Focus,
                "## Status\n+ Mission: Build and operate the Airbnb clone.\n+ Mission state: active\n+ Continuation mode: continuous\n+ Trigger intensity: hot\n## Blocker\n+ Current blocker: none\n## Next\n+ Next slice: implement the host onboarding flow\n## Done / Gate\n+ Done gate: do not close while the capability audit is still open\n+ Closure confidence: low\n",
            )
            .expect("failed to update focus");
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("service state poisoned");
            shared.last_progress_epoch_secs = current_epoch_secs().saturating_sub(90);
        }

        monitor_mission_continuity(&root, &state).expect("mission watcher should succeed");
        monitor_mission_continuity(&root, &state).expect("duplicate mission watcher should no-op");

        let tasks = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list queue tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].thread_key,
            mission_thread_key(turn_loop::CHAT_CONVERSATION_ID)
        );
        assert!(tasks[0].prompt.contains("Mission continuity watchdog"));
        let events = governance::list_recent_events(&root, turn_loop::CHAT_CONVERSATION_ID, 8)
            .expect("failed to list governance events");
        assert!(events
            .iter()
            .any(|event| event.mechanism_id == "mission_idle_watchdog"));
    }

    #[test]
    fn mission_watcher_skips_closed_mission() {
        let root = temp_root("ctox-mission-watcher-closed");
        std::fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let engine =
            lcm::LcmEngine::open(&root.join("runtime/ctox_lcm.db"), lcm::LcmConfig::default())
                .expect("failed to open lcm");
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .expect("failed to init continuity");
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                lcm::ContinuityKind::Focus,
                "## Status\n+ Mission: Build and operate the Airbnb clone.\n+ Mission state: done\n+ Continuation mode: closed\n+ Trigger intensity: archive\n## Blocker\n+ Current blocker: none\n## Next\n+ Next slice: none\n## Done / Gate\n+ Done gate: capability audit closed and automation stable\n+ Closure confidence: complete\n",
            )
            .expect("failed to update focus");
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("service state poisoned");
            shared.last_progress_epoch_secs = current_epoch_secs().saturating_sub(90);
        }

        monitor_mission_continuity(&root, &state).expect("mission watcher should succeed");

        let tasks = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list queue tasks");
        assert!(tasks.is_empty());
    }

    #[test]
    fn mission_watcher_respects_hard_runtime_blocker_cooldown() {
        let root = temp_root("ctox-mission-watcher-backoff");
        std::fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let engine =
            lcm::LcmEngine::open(&root.join("runtime/ctox_lcm.db"), lcm::LcmConfig::default())
                .expect("failed to open lcm");
        let _ = engine
            .continuity_init_documents(turn_loop::CHAT_CONVERSATION_ID)
            .expect("failed to init continuity");
        engine
            .continuity_apply_diff(
                turn_loop::CHAT_CONVERSATION_ID,
                lcm::ContinuityKind::Focus,
                "## Status\n+ Mission: Build and operate the Airbnb clone.\n+ Mission state: active\n+ Continuation mode: continuous\n+ Trigger intensity: hot\n## Blocker\n+ Current blocker: OPENAI quota exhausted.\n## Next\n+ Next slice: resume the marketplace core once inference is available again.\n## Done / Gate\n+ Done gate: do not close while the mission remains open.\n+ Closure confidence: low\n",
            )
            .expect("failed to update focus");
        let state = Arc::new(Mutex::new(SharedState::default()));
        {
            let mut shared = state.lock().expect("service state poisoned");
            shared.last_progress_epoch_secs = current_epoch_secs().saturating_sub(120);
            shared.last_error = Some(
                "CTOX chat could not continue because the configured OpenAI API quota is exhausted or billing is unavailable for the selected model.".to_string(),
            );
        }

        monitor_mission_continuity(&root, &state).expect("mission watcher should succeed");

        let tasks = channels::list_queue_tasks(&root, &["pending".to_string()], 10)
            .expect("failed to list queue tasks");
        assert!(tasks.is_empty());
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
                prompt: "Continue benchmark".to_string(),
                goal: "Continue benchmark".to_string(),
                preview: "Continue benchmark".to_string(),
                source_label: "queue".to_string(),
                suggested_skill: None,
                leased_message_keys: Vec::new(),
                leased_ticket_event_keys: Vec::new(),
                thread_key: Some("queue/mission-1".to_string()),
                workspace_root: None,
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
}
