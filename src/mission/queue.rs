use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use std::time::Duration;

use crate::channels;
use crate::execution::agent::direct_session::PersistentSession;
use crate::inference::runtime_env;
use crate::plan;
use crate::service::harness_flow::{
    record_harness_flow_event_lossy, RecordHarnessFlowEventRequest,
};
use crate::tickets;

const DEFAULT_LIST_LIMIT: usize = 20;
const DEFAULT_TICKET_SYSTEM: &str = "internal";
const SPILL_RESTORE_LEASE_OWNER: &str = "spill-restore-hold";
const SPILL_RESTORE_TITLE_PREFIX: &str = "spill restore: ";
const QUEUE_REPAIR_TIMEOUT_SECS: u64 = 300;
const QUEUE_REPAIR_SKILL_RELATIVE_PATH: &str =
    "skills/system/mission_orchestration/queue-cleanup/SKILL.md";

const QUEUE_REPAIR_SYSTEM_PROMPT: &str = r#"You are CTOX Queue Repair.

You run a dedicated external queue-repair pass.
This is not normal execution. Gather facts yourself from the runtime SQLite store, queue state,
self-work and ticket state, active strategic directives, founder or owner communication state,
review state, and service status.

Use the queue-cleanup skill first and follow it.

Primary goals:
- preserve the canonical mission hot path
- stop stale, duplicate, superseded, or flooding queue work
- keep founder or owner visible work separate from internal queue churn
- avoid deleting valid work when block, release, or reprioritize would be safer

Do not mutate the queue directly from this run.
Return a repair plan only. The caller will apply the plan deterministically.

Use exact output format:

STATE: READY|NOOP|BLOCKED|PARTIAL
SUMMARY: <one sentence>
CANONICAL_HOT_PATH:
- <message_key or thread> :: <why>
STALE_QUEUE_ITEMS:
- <message_key> :: <why>
SURVIVING_QUEUE_ITEMS:
- <message_key> :: <why>
REPAIR_ACTIONS:
- cancel <message_key> :: <reason>
- block <message_key> :: <reason>
- release <message_key> :: <reason>
- reprioritize <message_key> <urgent|high|normal|low> :: <reason>
- none
EVIDENCE:
- <check> => <result>
HANDOFF:
- <only when another queue repair pass should continue; otherwise write "none">
"#;

const QUEUE_REPAIR_VERIFY_SYSTEM_PROMPT: &str = r#"You are CTOX Queue Repair Verification.

You verify a queue repair after deterministic actions were applied.
Gather facts yourself from the runtime SQLite store, queue state, strategic directives,
founder or owner communication state, and service status.

Use the queue-cleanup skill first and follow it.

Decide whether the queue is now stable enough to resume the canonical hot path.

Use exact output format:

STATE: STABLE|UNSTABLE|PARTIAL
SUMMARY: <one sentence>
CANONICAL_HOT_PATH:
- <message_key or thread> :: <why>
REMAINING_RISKS:
- <risk or "none">
FOLLOW_UP_ACTIONS:
- <action or "none">
EVIDENCE:
- <check> => <result>
HANDOFF:
- <only when another verification pass should continue; otherwise write "none">
"#;

#[derive(Debug, Clone, Serialize)]
struct QueueTicketBridgeView {
    message_key: String,
    work_id: String,
    ticket_system: String,
    bridge_state: String,
    spilled_at: String,
    restored_at: Option<String>,
    task: channels::QueueTaskView,
    ticket: tickets::TicketSelfWorkItemView,
}

#[derive(Debug, Clone, Serialize)]
struct QueueTicketBridgeListItem {
    message_key: String,
    work_id: String,
    ticket_system: String,
    bridge_state: String,
    spilled_at: String,
    restored_at: Option<String>,
    task: Option<channels::QueueTaskView>,
    ticket: Option<tickets::TicketSelfWorkItemView>,
}

#[derive(Debug, Clone, Serialize)]
struct QueueSpillCandidateView {
    message_key: String,
    priority: String,
    route_status: String,
    title: String,
    thread_key: String,
    suggested_skill: Option<String>,
    workspace_root: Option<String>,
    candidate_score: i64,
    recommendation: String,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct QueueRepairView {
    stale_plan_routes_repaired: usize,
    open_queue_count: usize,
    open_queue_preview: Vec<channels::QueueTaskView>,
    agentic: Option<AgenticQueueRepairView>,
}

#[derive(Debug, Clone, Serialize)]
struct AgenticQueueRepairView {
    state: String,
    summary: String,
    canonical_hot_path: Vec<String>,
    stale_queue_items: Vec<String>,
    surviving_queue_items: Vec<String>,
    repair_actions: Vec<QueueRepairActionView>,
    evidence: Vec<String>,
    handoff: Option<String>,
    applied_actions: Vec<QueueRepairActionView>,
    verification: Option<QueueRepairVerificationView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct QueueRepairActionView {
    action: String,
    message_key: String,
    priority: Option<String>,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Default)]
struct QueueRepairVerificationView {
    state: String,
    summary: String,
    canonical_hot_path: Vec<String>,
    remaining_risks: Vec<String>,
    follow_up_actions: Vec<String>,
    evidence: Vec<String>,
    handoff: Option<String>,
}

pub fn handle_queue_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "add" => {
            let title = required_flag_value(args, "--title")
                .context("usage: ctox queue add --title <label> --prompt <text> [--thread-key <key>] [--workspace-root <path>] [--skill <name>] [--priority <urgent|high|normal|low>] [--parent-message-key <key>]")?;
            let prompt = required_flag_value(args, "--prompt")
                .context("usage: ctox queue add --title <label> --prompt <text> [--thread-key <key>] [--workspace-root <path>] [--skill <name>] [--priority <urgent|high|normal|low>] [--parent-message-key <key>]")?;
            let thread_key = find_flag_value(args, "--thread-key")
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| default_thread_key(title));
            let task = channels::create_queue_task(
                root,
                channels::QueueTaskCreateRequest {
                    title: title.to_string(),
                    prompt: prompt.to_string(),
                    thread_key,
                    workspace_root: find_flag_value(args, "--workspace-root")
                        .map(ToOwned::to_owned)
                        .or_else(|| channels::legacy_workspace_root_from_prompt(prompt)),
                    priority: find_flag_value(args, "--priority")
                        .unwrap_or("normal")
                        .to_string(),
                    suggested_skill: find_flag_value(args, "--skill").map(ToOwned::to_owned),
                    parent_message_key: find_flag_value(args, "--parent-message-key")
                        .map(ToOwned::to_owned),
                    extra_metadata: None,
                },
            )?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "list" => {
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_LIST_LIMIT);
            let statuses = collect_flag_values(args, "--status");
            let tasks = channels::list_queue_tasks(root, &statuses, limit)?;
            print_json(&json!({
                "ok": true,
                "count": tasks.len(),
                "tasks": tasks,
            }))
        }
        "show" => {
            let message_key = required_flag_value(args, "--message-key")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox queue show --message-key <key>")?;
            let task =
                channels::load_queue_task(root, message_key)?.context("queue task not found")?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "edit" => {
            let message_key = required_flag_value(args, "--message-key")
                .context("usage: ctox queue edit --message-key <key> [--title <label>] [--prompt <text>] [--thread-key <key>] [--workspace-root <path>] [--clear-workspace-root] [--skill <name>] [--clear-skill] [--priority <urgent|high|normal|low>]")?;
            ensure_edit_requested(
                args,
                &[
                    "--title",
                    "--prompt",
                    "--thread-key",
                    "--workspace-root",
                    "--skill",
                    "--priority",
                ],
                &["--clear-skill", "--clear-workspace-root"],
            )?;
            let task = channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: message_key.to_string(),
                    title: find_flag_value(args, "--title").map(ToOwned::to_owned),
                    prompt: find_flag_value(args, "--prompt").map(ToOwned::to_owned),
                    thread_key: find_flag_value(args, "--thread-key").map(ToOwned::to_owned),
                    workspace_root: find_flag_value(args, "--workspace-root")
                        .map(ToOwned::to_owned),
                    clear_workspace_root: args.iter().any(|arg| arg == "--clear-workspace-root"),
                    priority: find_flag_value(args, "--priority").map(ToOwned::to_owned),
                    suggested_skill: find_flag_value(args, "--skill").map(ToOwned::to_owned),
                    clear_skill: args.iter().any(|arg| arg == "--clear-skill"),
                    route_status: None,
                    status_note: None,
                    clear_note: false,
                },
            )?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "reprioritize" => {
            let message_key = required_flag_value(args, "--message-key")
                .context("usage: ctox queue reprioritize --message-key <key> --priority <urgent|high|normal|low>")?;
            let priority = required_flag_value(args, "--priority")
                .context("usage: ctox queue reprioritize --message-key <key> --priority <urgent|high|normal|low>")?;
            let task = channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: message_key.to_string(),
                    priority: Some(priority.to_string()),
                    ..Default::default()
                },
            )?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "block" => {
            let message_key = required_flag_value(args, "--message-key")
                .context("usage: ctox queue block --message-key <key> --reason <text>")?;
            let reason = required_flag_value(args, "--reason")
                .context("usage: ctox queue block --message-key <key> --reason <text>")?;
            let task = channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: message_key.to_string(),
                    route_status: Some("blocked".to_string()),
                    status_note: Some(reason.to_string()),
                    ..Default::default()
                },
            )?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "release" => {
            let message_key = required_flag_value(args, "--message-key")
                .context("usage: ctox queue release --message-key <key> [--priority <urgent|high|normal|low>] [--clear-note] [--note <text>]")?;
            let task = channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: message_key.to_string(),
                    priority: find_flag_value(args, "--priority").map(ToOwned::to_owned),
                    route_status: Some("pending".to_string()),
                    status_note: find_flag_value(args, "--note").map(ToOwned::to_owned),
                    clear_note: args.iter().any(|arg| arg == "--clear-note")
                        || find_flag_value(args, "--note").is_none(),
                    ..Default::default()
                },
            )?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "complete" => {
            let message_key = required_flag_value(args, "--message-key")
                .context("usage: ctox queue complete --message-key <key> [--note <text>]")?;
            let task = channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: message_key.to_string(),
                    route_status: Some("handled".to_string()),
                    status_note: find_flag_value(args, "--note").map(ToOwned::to_owned),
                    clear_note: find_flag_value(args, "--note").is_none(),
                    ..Default::default()
                },
            )?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "fail" => {
            let message_key = required_flag_value(args, "--message-key")
                .context("usage: ctox queue fail --message-key <key> --reason <text>")?;
            let reason = required_flag_value(args, "--reason")
                .context("usage: ctox queue fail --message-key <key> --reason <text>")?;
            let task = channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: message_key.to_string(),
                    route_status: Some("failed".to_string()),
                    status_note: Some(reason.to_string()),
                    ..Default::default()
                },
            )?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "cancel" => {
            let message_key = required_flag_value(args, "--message-key")
                .context("usage: ctox queue cancel --message-key <key> [--reason <text>]")?;
            let task = channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: message_key.to_string(),
                    route_status: Some("cancelled".to_string()),
                    status_note: find_flag_value(args, "--reason").map(ToOwned::to_owned),
                    clear_note: find_flag_value(args, "--reason").is_none(),
                    ..Default::default()
                },
            )?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "spill" => {
            let message_key = required_flag_value(args, "--message-key")
                .context("usage: ctox queue spill --message-key <key> [--ticket-system <name>] [--reason <text>] [--skill <name>] [--publish]")?;
            let bridge = spill_queue_task_to_ticket(
                root,
                message_key,
                find_flag_value(args, "--ticket-system").unwrap_or(DEFAULT_TICKET_SYSTEM),
                find_flag_value(args, "--reason"),
                find_flag_value(args, "--skill"),
                args.iter().any(|arg| arg == "--publish"),
            )?;
            print_json(&json!({"ok": true, "bridge": bridge}))
        }
        "spill-candidates" => {
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_LIST_LIMIT);
            let candidates = list_queue_spill_candidates(root, limit)?;
            print_json(&json!({"ok": true, "count": candidates.len(), "candidates": candidates}))
        }
        "spills" => {
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_LIST_LIMIT);
            let bridges = list_queue_ticket_bridges(root, find_flag_value(args, "--state"), limit)?;
            print_json(&json!({"ok": true, "count": bridges.len(), "spills": bridges}))
        }
        "restore" => {
            let message_key = required_flag_value(args, "--message-key")
                .context("usage: ctox queue restore --message-key <key> [--priority <urgent|high|normal|low>] [--note <text>]")?;
            let bridge = restore_spilled_queue_task(
                root,
                message_key,
                find_flag_value(args, "--priority"),
                find_flag_value(args, "--note"),
            )?;
            print_json(&json!({"ok": true, "bridge": bridge}))
        }
        "repair" => {
            let repaired = repair_queue_state(
                root,
                args.iter().any(|arg| arg == "--mechanical"),
                args.iter().any(|arg| arg == "--dry-run"),
            )?;
            print_json(&json!({"ok": true, "repair": repaired}))
        }
        _ => anyhow::bail!(
            "usage:\n  ctox queue add --title <label> --prompt <text> [--thread-key <key>] [--workspace-root <path>] [--skill <name>] [--priority <urgent|high|normal|low>] [--parent-message-key <key>]\n  ctox queue list [--status <pending|leased|blocked|failed|handled|cancelled>]... [--limit <n>]\n  ctox queue show --message-key <key>\n  ctox queue edit --message-key <key> [--title <label>] [--prompt <text>] [--thread-key <key>] [--workspace-root <path>] [--clear-workspace-root] [--skill <name>] [--clear-skill] [--priority <urgent|high|normal|low>]\n  ctox queue reprioritize --message-key <key> --priority <urgent|high|normal|low>\n  ctox queue block --message-key <key> --reason <text>\n  ctox queue release --message-key <key> [--priority <urgent|high|normal|low>] [--clear-note] [--note <text>]\n  ctox queue complete --message-key <key> [--note <text>]\n  ctox queue fail --message-key <key> --reason <text>\n  ctox queue cancel --message-key <key> [--reason <text>]\n  ctox queue spill --message-key <key> [--ticket-system <name>] [--reason <text>] [--skill <name>] [--publish]\n  ctox queue spill-candidates [--limit <n>]\n  ctox queue spills [--state <spilled|restored>] [--limit <n>]\n  ctox queue restore --message-key <key> [--priority <urgent|high|normal|low>] [--note <text>]\n  ctox queue repair [--dry-run] [--mechanical]"
        ),
    }
}

fn repair_queue_state(
    root: &Path,
    mechanical_only: bool,
    dry_run: bool,
) -> Result<QueueRepairView> {
    let stale_plan_routes_repaired = plan::repair_stale_step_routing_state(root)?;
    let agentic = if mechanical_only {
        None
    } else {
        Some(run_agentic_queue_repair(root, dry_run)?)
    };
    let open_statuses = vec![
        "pending".to_string(),
        "leased".to_string(),
        "blocked".to_string(),
    ];
    let open_queue_preview = channels::list_queue_tasks(root, &open_statuses, 20)?;
    let open_queue_count = open_queue_preview.len();
    Ok(QueueRepairView {
        stale_plan_routes_repaired,
        open_queue_count,
        open_queue_preview,
        agentic,
    })
}

fn run_agentic_queue_repair(root: &Path, dry_run: bool) -> Result<AgenticQueueRepairView> {
    let settings = runtime_env::effective_runtime_env_map(root).unwrap_or_default();
    let report = run_queue_repair_agent(
        root,
        &settings,
        QUEUE_REPAIR_SYSTEM_PROMPT,
        &build_queue_repair_prompt(root, dry_run),
    )?;
    let parsed = parse_queue_repair_report(&report);
    let applied_actions = if dry_run {
        Vec::new()
    } else {
        apply_queue_repair_actions(root, &parsed.repair_actions)?
    };
    let verification = if dry_run {
        None
    } else {
        let verify_report = run_queue_repair_agent(
            root,
            &settings,
            QUEUE_REPAIR_VERIFY_SYSTEM_PROMPT,
            &build_queue_repair_verify_prompt(root, &report, &applied_actions),
        )?;
        Some(parse_queue_repair_verification(&verify_report))
    };

    Ok(AgenticQueueRepairView {
        state: parsed.state,
        summary: parsed.summary,
        canonical_hot_path: parsed.canonical_hot_path,
        stale_queue_items: parsed.stale_queue_items,
        surviving_queue_items: parsed.surviving_queue_items,
        repair_actions: parsed.repair_actions,
        evidence: parsed.evidence,
        handoff: parsed.handoff,
        applied_actions,
        verification,
    })
}

fn run_queue_repair_agent(
    root: &Path,
    settings: &std::collections::BTreeMap<String, String>,
    system_prompt: &str,
    prompt: &str,
) -> Result<String> {
    let mut session =
        PersistentSession::start_with_instructions(root, settings, Some(system_prompt), true)?;
    let report = session.run_turn(
        prompt,
        Some(Duration::from_secs(QUEUE_REPAIR_TIMEOUT_SECS)),
        None,
        Some(false),
        0,
    )?;
    session.shutdown();
    Ok(report)
}

fn build_queue_repair_prompt(root: &Path, dry_run: bool) -> String {
    let skill_path = root.join(QUEUE_REPAIR_SKILL_RELATIVE_PATH);
    let skill = if skill_path.exists() {
        skill_path.to_string_lossy().into_owned()
    } else {
        "(missing)".to_string()
    };
    let runtime_db_path = root.join("runtime/ctox.sqlite3");
    let harness_block = render_confirmed_harness_findings_block(root);
    format!(
        "== QUEUE REPAIR ASSIGNMENT ==\n\
\n\
Workspace root: {}\n\
Runtime DB: {}\n\
Queue cleanup skill: {}\n\
Dry run: {}\n\
{}\
Open the queue cleanup skill first and follow it.\n\
\n\
Gather the queue-repair facts yourself from the runtime SQLite store, queue state, ticket/self-work state, active strategic directives, founder or owner communication threads, review findings, and service status.\n\
\n\
Required work:\n\
1. identify the canonical hot path that should survive\n\
2. identify stale, duplicate, superseded, or contaminated queue work\n\
3. pay special attention to founder or owner communication drift contaminating queue work\n\
4. for every confirmed harness finding listed above whose entity touches the queue scope, decide: block the entity (with `ctox queue block --reason \"harness-mining: ...\"`) and then `ctox harness-mining finding-mitigate --finding-id <id> --by agent --note \"<what was done>\"`. Do NOT release a queue item that appears as a confirmed stuck-case finding without mitigating the finding first.\n\
5. propose the minimum safe queue actions needed to recover focus\n\
6. do not mutate anything directly; only return the repair plan\n",
        root.display(),
        runtime_db_path.display(),
        skill,
        if dry_run { "yes" } else { "no" },
        harness_block,
    )
}

fn render_confirmed_harness_findings_block(root: &Path) -> String {
    // Read-only peek into ctox_hm_findings before constructing the prompt.
    // If the audit-tick has not run yet (table missing) or the read fails
    // for any reason we silently skip the block — the agent then falls
    // back to gathering signals via `ctox harness-mining findings` itself.
    let db_path = root.join("runtime/ctox.sqlite3");
    let conn =
        match Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(c) => c,
            Err(_) => return String::new(),
        };
    let table_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ctox_hm_findings'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if table_exists == 0 {
        return String::new();
    }
    let findings =
        match crate::service::harness_mining::findings::list(&conn, Some("confirmed"), None, 15) {
            Ok(rows) => rows,
            Err(_) => return String::new(),
        };
    if findings.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\nConfirmed harness-mining findings (mitigate or acknowledge before resuming protected work):\n",
    );
    for f in &findings {
        let id = f
            .get("finding_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let kind = f
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let severity = f
            .get("severity")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let entity_type = f
            .get("entity_type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let entity_id = f
            .get("entity_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let lane = f
            .get("lane")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        out.push_str(&format!(
            "- [{severity}] {kind} :: {id} (entity_type={entity_type}, lane={lane})"
        ));
        if !entity_id.is_empty() {
            out.push_str(&format!(" entity_id={entity_id}"));
        }
        out.push('\n');
    }
    out.push('\n');
    out
}

fn build_queue_repair_verify_prompt(
    root: &Path,
    prior_report: &str,
    applied_actions: &[QueueRepairActionView],
) -> String {
    let skill_path = root.join(QUEUE_REPAIR_SKILL_RELATIVE_PATH);
    let skill = if skill_path.exists() {
        skill_path.to_string_lossy().into_owned()
    } else {
        "(missing)".to_string()
    };
    let runtime_db_path = root.join("runtime/ctox.sqlite3");
    let mut actions_rendered = String::new();
    if applied_actions.is_empty() {
        actions_rendered.push_str("- none\n");
    } else {
        for action in applied_actions {
            let priority = action
                .priority
                .as_deref()
                .map(|value| format!(" {value}"))
                .unwrap_or_default();
            actions_rendered.push_str(&format!(
                "- {} {}{} :: {}\n",
                action.action, action.message_key, priority, action.reason
            ));
        }
    }
    format!(
        "== QUEUE REPAIR VERIFICATION ==\n\
\n\
Workspace root: {}\n\
Runtime DB: {}\n\
Queue cleanup skill: {}\n\
\n\
Open the queue cleanup skill first and follow it.\n\
\n\
Prior repair report:\n\
{}\n\
\n\
Applied actions:\n\
{}\n\
Verify the current queue state and judge whether the canonical hot path is now clear enough to resume.\n",
        root.display(),
        runtime_db_path.display(),
        skill,
        prior_report.trim(),
        actions_rendered
    )
}

#[derive(Debug, Clone, Default)]
struct ParsedQueueRepairReport {
    state: String,
    summary: String,
    canonical_hot_path: Vec<String>,
    stale_queue_items: Vec<String>,
    surviving_queue_items: Vec<String>,
    repair_actions: Vec<QueueRepairActionView>,
    evidence: Vec<String>,
    handoff: Option<String>,
}

fn parse_queue_repair_report(report: &str) -> ParsedQueueRepairReport {
    ParsedQueueRepairReport {
        state: parse_prefixed_line(report, "STATE:")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "PARTIAL".to_string()),
        summary: parse_prefixed_line(report, "SUMMARY:")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| clip_text(report, 220)),
        canonical_hot_path: parse_section_items(report, "CANONICAL_HOT_PATH:"),
        stale_queue_items: parse_section_items(report, "STALE_QUEUE_ITEMS:"),
        surviving_queue_items: parse_section_items(report, "SURVIVING_QUEUE_ITEMS:"),
        repair_actions: parse_queue_repair_actions(report),
        evidence: parse_section_items(report, "EVIDENCE:"),
        handoff: parse_handoff_block(report),
    }
}

fn parse_queue_repair_verification(report: &str) -> QueueRepairVerificationView {
    QueueRepairVerificationView {
        state: parse_prefixed_line(report, "STATE:")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "PARTIAL".to_string()),
        summary: parse_prefixed_line(report, "SUMMARY:")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| clip_text(report, 220)),
        canonical_hot_path: parse_section_items(report, "CANONICAL_HOT_PATH:"),
        remaining_risks: parse_section_items(report, "REMAINING_RISKS:"),
        follow_up_actions: parse_section_items(report, "FOLLOW_UP_ACTIONS:"),
        evidence: parse_section_items(report, "EVIDENCE:"),
        handoff: parse_handoff_block(report),
    }
}

fn parse_queue_repair_actions(report: &str) -> Vec<QueueRepairActionView> {
    parse_section_items(report, "REPAIR_ACTIONS:")
        .into_iter()
        .filter_map(|item| parse_queue_repair_action_line(&item))
        .collect()
}

fn parse_queue_repair_action_line(value: &str) -> Option<QueueRepairActionView> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }
    let (command_part, reason_part) = trimmed.rsplit_once("::")?;
    let reason = reason_part.trim();
    if reason.is_empty() {
        return None;
    }
    let mut tokens = command_part.split_whitespace();
    let action = tokens.next()?.trim().to_ascii_lowercase();
    let message_key = tokens.next()?.trim().to_string();
    let priority = if action == "reprioritize" {
        Some(tokens.next()?.trim().to_string())
    } else {
        None
    };
    match action.as_str() {
        "cancel" | "block" | "release" | "reprioritize" | "complete" => {
            Some(QueueRepairActionView {
                action,
                message_key,
                priority,
                reason: reason.to_string(),
            })
        }
        _ => None,
    }
}

fn apply_queue_repair_actions(
    root: &Path,
    actions: &[QueueRepairActionView],
) -> Result<Vec<QueueRepairActionView>> {
    let mut applied = Vec::new();
    for action in actions {
        if channels::load_queue_task(root, &action.message_key)?.is_none() {
            continue;
        }
        match action.action.as_str() {
            "cancel" => {
                let _ = channels::update_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: action.message_key.clone(),
                        route_status: Some("cancelled".to_string()),
                        status_note: Some(action.reason.clone()),
                        ..Default::default()
                    },
                )?;
            }
            "block" => {
                let _ = channels::update_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: action.message_key.clone(),
                        route_status: Some("blocked".to_string()),
                        status_note: Some(action.reason.clone()),
                        ..Default::default()
                    },
                )?;
            }
            "release" => {
                let _ = channels::update_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: action.message_key.clone(),
                        route_status: Some("pending".to_string()),
                        status_note: Some(action.reason.clone()),
                        ..Default::default()
                    },
                )?;
            }
            "reprioritize" => {
                let Some(priority) = action.priority.clone() else {
                    continue;
                };
                let _ = channels::update_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: action.message_key.clone(),
                        priority: Some(priority),
                        status_note: Some(action.reason.clone()),
                        ..Default::default()
                    },
                )?;
            }
            "complete" => {
                let _ = channels::update_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: action.message_key.clone(),
                        route_status: Some("handled".to_string()),
                        status_note: Some(action.reason.clone()),
                        ..Default::default()
                    },
                )?;
            }
            _ => continue,
        }
        applied.push(action.clone());
    }
    Ok(applied)
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
            if trimmed.ends_with(':') && !trimmed.starts_with("- ") && trimmed != header {
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
            if trimmed.ends_with(':') && !trimmed.starts_with("- ") {
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

fn spill_queue_task_to_ticket(
    root: &Path,
    message_key: &str,
    ticket_system: &str,
    reason: Option<&str>,
    explicit_skill: Option<&str>,
    publish: bool,
) -> Result<QueueTicketBridgeView> {
    let task = channels::load_queue_task(root, message_key)?.context("queue task not found")?;
    let existing = load_queue_ticket_bridge(root, message_key)?;
    if let Some(existing) = existing.filter(|bridge| bridge.bridge_state == "spilled") {
        let ticket = tickets::load_ticket_self_work_item(root, &existing.work_id)?
            .context("bridged ticket self-work item missing")?;
        return Ok(QueueTicketBridgeView {
            message_key: existing.message_key,
            work_id: existing.work_id,
            ticket_system: existing.ticket_system,
            bridge_state: existing.bridge_state,
            spilled_at: existing.spilled_at,
            restored_at: existing.restored_at,
            task,
            ticket,
        });
    }

    let effective_skill = explicit_skill
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or(task.suggested_skill.clone())
        .or_else(|| Some("queue-orchestrator".to_string()));
    let ticket = tickets::put_ticket_self_work_item(
        root,
        tickets::TicketSelfWorkUpsertInput {
            source_system: ticket_system.trim().to_string(),
            kind: "queue-overflow".to_string(),
            title: format!("Queue spill: {}", task.title),
            body_text: render_queue_spill_body(&task, reason),
            state: "spilled".to_string(),
            metadata: json!({
                "skill": effective_skill,
                "dedupe_key": task.message_key,
                "bridge_kind": "queue_spillover",
                "queue_message_key": task.message_key,
                "queue_thread_key": task.thread_key,
                "queue_priority": task.priority,
                "queue_workspace_root": task.workspace_root,
                "queue_parent_message_key": task.parent_message_key,
                "queue_task_title": task.title,
                "queue_prompt": task.prompt,
                "reason": reason.map(str::trim).filter(|value| !value.is_empty()),
            }),
        },
        publish,
    )?;
    let note = match reason.map(str::trim).filter(|value| !value.is_empty()) {
        Some(reason) => format!("spilled to ticket {}: {reason}", ticket.work_id),
        None => format!("spilled to ticket {}", ticket.work_id),
    };
    let task = channels::update_queue_task(
        root,
        channels::QueueTaskUpdateRequest {
            message_key: task.message_key.clone(),
            route_status: Some("blocked".to_string()),
            status_note: Some(note),
            ..Default::default()
        },
    )?;
    let bridge = upsert_queue_ticket_bridge(
        root,
        QueueTicketBridgeRecord {
            message_key: task.message_key.clone(),
            work_id: ticket.work_id.clone(),
            ticket_system: ticket.source_system.clone(),
            bridge_state: "spilled".to_string(),
            spilled_at: now_iso_string(),
            restored_at: None,
        },
    )?;
    let _ = ensure_spill_restore_follow_up(root, &task, &ticket.work_id, reason)?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "queue.spilled_to_ticket",
            title: "Queue item moved to ticket backlog",
            body_text: reason
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Queue item was moved out of the active queue."),
            message_key: Some(&task.message_key),
            work_id: Some(&ticket.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "ticket_system": ticket.source_system,
                "ticket_state": ticket.state,
                "queue_priority": task.priority,
                "queue_status": task.route_status,
            }),
        },
    );
    Ok(QueueTicketBridgeView {
        message_key: bridge.message_key,
        work_id: bridge.work_id,
        ticket_system: bridge.ticket_system,
        bridge_state: bridge.bridge_state,
        spilled_at: bridge.spilled_at,
        restored_at: bridge.restored_at,
        task,
        ticket,
    })
}

fn restore_spilled_queue_task(
    root: &Path,
    message_key: &str,
    priority: Option<&str>,
    note: Option<&str>,
) -> Result<QueueTicketBridgeView> {
    let bridge = load_queue_ticket_bridge(root, message_key)?
        .context("queue task is not currently linked to a spilled ticket")?;
    anyhow::ensure!(
        bridge.bridge_state == "spilled",
        "queue task is not in spilled state"
    );
    let current_task =
        channels::load_queue_task(root, message_key)?.context("queue task not found")?;
    let restored_title = restored_queue_follow_up_title(&current_task.title);
    let restored_note = note
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            format!(
                "restored from ticket {}; held as the single open follow-up for the next turn",
                bridge.work_id
            )
        });
    let _ = channels::update_queue_task(
        root,
        channels::QueueTaskUpdateRequest {
            message_key: message_key.to_string(),
            title: Some(restored_title),
            priority: priority.map(ToOwned::to_owned),
            status_note: Some(restored_note.clone()),
            ..Default::default()
        },
    )?;
    complete_spill_restore_follow_ups(root, message_key)?;
    let task = channels::lease_queue_task(root, message_key, SPILL_RESTORE_LEASE_OWNER)?;
    let ticket = tickets::set_ticket_self_work_state(root, &bridge.work_id, "restored")?;
    let bridge = upsert_queue_ticket_bridge(
        root,
        QueueTicketBridgeRecord {
            message_key: bridge.message_key.clone(),
            work_id: bridge.work_id.clone(),
            ticket_system: bridge.ticket_system.clone(),
            bridge_state: "restored".to_string(),
            spilled_at: bridge.spilled_at,
            restored_at: Some(now_iso_string()),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "queue.restored_from_ticket",
            title: "Ticket backlog restored to queue",
            body_text: &restored_note,
            message_key: Some(message_key),
            work_id: Some(&ticket.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "ticket_system": ticket.source_system,
                "ticket_state": ticket.state,
                "queue_priority": task.priority,
                "queue_status": task.route_status,
            }),
        },
    );
    Ok(QueueTicketBridgeView {
        message_key: bridge.message_key,
        work_id: bridge.work_id,
        ticket_system: bridge.ticket_system,
        bridge_state: bridge.bridge_state,
        spilled_at: bridge.spilled_at,
        restored_at: bridge.restored_at,
        task,
        ticket,
    })
}

#[derive(Debug, Clone)]
struct QueueTicketBridgeRecord {
    message_key: String,
    work_id: String,
    ticket_system: String,
    bridge_state: String,
    spilled_at: String,
    restored_at: Option<String>,
}

fn spill_restore_follow_up_title(current_title: &str) -> String {
    let trimmed = current_title.trim();
    if trimmed.is_empty() {
        format!("{SPILL_RESTORE_TITLE_PREFIX}spilled queue task")
    } else {
        format!("{SPILL_RESTORE_TITLE_PREFIX}{trimmed}")
    }
}

fn render_spill_restore_follow_up_prompt(
    task: &channels::QueueTaskView,
    work_id: &str,
    reason: Option<&str>,
) -> String {
    let mut lines = vec![
        "Restore the spilled queue task from internal ticket self-work when the queue is ready."
            .to_string(),
        format!("Original queue task: {}", task.title.trim()),
        format!("Queue message key: {}", task.message_key),
        format!("Ticket self-work id: {work_id}"),
        "Required actions:".to_string(),
        "- confirm the spill is still the right choice".to_string(),
        "- restore the original queue task with `ctox queue restore --message-key <key>`"
            .to_string(),
        "- keep exactly one open CTOX follow-up after restore".to_string(),
    ];
    if let Some(reason) = reason.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(format!("Spill reason: {reason}"));
    }
    lines.join("\n")
}

fn ensure_spill_restore_follow_up(
    root: &Path,
    task: &channels::QueueTaskView,
    work_id: &str,
    reason: Option<&str>,
) -> Result<channels::QueueTaskView> {
    let existing = channels::list_queue_tasks(root, &[], 256)?
        .into_iter()
        .find(|candidate| {
            candidate.parent_message_key.as_deref() == Some(task.message_key.as_str())
                && candidate
                    .title
                    .to_ascii_lowercase()
                    .starts_with(SPILL_RESTORE_TITLE_PREFIX)
        });
    let follow_up = if let Some(existing) = existing {
        channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: existing.message_key,
                title: Some(spill_restore_follow_up_title(&task.title)),
                prompt: Some(render_spill_restore_follow_up_prompt(task, work_id, reason)),
                priority: Some("high".to_string()),
                suggested_skill: task.suggested_skill.clone(),
                ..Default::default()
            },
        )?
    } else {
        channels::create_queue_task(
            root,
            channels::QueueTaskCreateRequest {
                title: spill_restore_follow_up_title(&task.title),
                prompt: render_spill_restore_follow_up_prompt(task, work_id, reason),
                thread_key: task.thread_key.clone(),
                workspace_root: task.workspace_root.clone(),
                priority: "high".to_string(),
                suggested_skill: task.suggested_skill.clone(),
                parent_message_key: Some(task.message_key.clone()),
                extra_metadata: None,
            },
        )?
    };
    channels::lease_queue_task(root, &follow_up.message_key, SPILL_RESTORE_LEASE_OWNER)
}

fn complete_spill_restore_follow_ups(root: &Path, parent_message_key: &str) -> Result<()> {
    for follow_up in channels::list_queue_tasks(root, &[], 256)?
        .into_iter()
        .filter(|task| {
            task.parent_message_key.as_deref() == Some(parent_message_key)
                && task
                    .title
                    .to_ascii_lowercase()
                    .starts_with(SPILL_RESTORE_TITLE_PREFIX)
                && task.route_status != "handled"
        })
    {
        let _ = channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: follow_up.message_key,
                route_status: Some("handled".to_string()),
                status_note: Some("superseded by the restored original queue task".to_string()),
                ..Default::default()
            },
        )?;
    }
    Ok(())
}

fn restored_queue_follow_up_title(current_title: &str) -> String {
    let trimmed = current_title.trim();
    let normalized = trimmed.to_ascii_lowercase();
    if normalized.starts_with("restored queue") || normalized.starts_with("restored follow-up") {
        return trimmed.to_string();
    }
    if trimmed.is_empty() {
        "restored queue rehydrate follow-up".to_string()
    } else {
        format!("restored queue rehydrate follow-up: {trimmed}")
    }
}

fn render_queue_spill_body(task: &channels::QueueTaskView, reason: Option<&str>) -> String {
    let mut lines = vec![
        "This task was spilled out of the CTOX queue into the internal ticket system.".to_string(),
        format!("Queue task: {}", task.title),
        format!("Queue message key: {}", task.message_key),
        format!("Thread: {}", task.thread_key),
        format!("Priority: {}", task.priority),
    ];
    if let Some(workspace_root) = task.workspace_root.as_deref() {
        lines.push(format!("Workspace: {}", workspace_root));
    }
    if let Some(skill) = task.suggested_skill.as_deref() {
        lines.push(format!("Suggested skill: {}", skill));
    }
    if let Some(parent) = task.parent_message_key.as_deref() {
        lines.push(format!("Parent queue message: {}", parent));
    }
    if let Some(reason) = reason.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(String::new());
        lines.push(format!("Spill reason: {}", reason));
    }
    lines.push(String::new());
    lines.push("Original prompt:".to_string());
    lines.push(task.prompt.clone());
    lines.join("\n")
}

fn queue_bridge_db_path(root: &Path) -> std::path::PathBuf {
    root.join("runtime/ctox.sqlite3")
}

fn open_queue_bridge_db(root: &Path) -> Result<Connection> {
    let path = queue_bridge_db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open queue bridge db {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    ensure_queue_bridge_schema(&conn)?;
    Ok(conn)
}

fn ensure_queue_bridge_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS queue_ticket_spills (
            message_key TEXT PRIMARY KEY,
            work_id TEXT NOT NULL,
            ticket_system TEXT NOT NULL,
            bridge_state TEXT NOT NULL,
            spilled_at TEXT NOT NULL,
            restored_at TEXT,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_queue_ticket_spills_work
            ON queue_ticket_spills(work_id, updated_at DESC);
        "#,
    )?;
    Ok(())
}

fn upsert_queue_ticket_bridge(
    root: &Path,
    record: QueueTicketBridgeRecord,
) -> Result<QueueTicketBridgeRecord> {
    let conn = open_queue_bridge_db(root)?;
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO queue_ticket_spills (
            message_key, work_id, ticket_system, bridge_state, spilled_at, restored_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(message_key) DO UPDATE SET
            work_id=excluded.work_id,
            ticket_system=excluded.ticket_system,
            bridge_state=excluded.bridge_state,
            spilled_at=excluded.spilled_at,
            restored_at=excluded.restored_at,
            updated_at=excluded.updated_at
        "#,
        params![
            record.message_key,
            record.work_id,
            record.ticket_system,
            record.bridge_state,
            record.spilled_at,
            record.restored_at,
            now,
        ],
    )?;
    load_queue_ticket_bridge(root, &record.message_key)?
        .context("queue ticket bridge missing after write")
}

fn load_queue_ticket_bridge(
    root: &Path,
    message_key: &str,
) -> Result<Option<QueueTicketBridgeRecord>> {
    let conn = open_queue_bridge_db(root)?;
    conn.query_row(
        r#"
        SELECT message_key, work_id, ticket_system, bridge_state, spilled_at, restored_at
        FROM queue_ticket_spills
        WHERE message_key = ?1
        LIMIT 1
        "#,
        params![message_key],
        |row| {
            Ok(QueueTicketBridgeRecord {
                message_key: row.get(0)?,
                work_id: row.get(1)?,
                ticket_system: row.get(2)?,
                bridge_state: row.get(3)?,
                spilled_at: row.get(4)?,
                restored_at: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn list_queue_ticket_bridges(
    root: &Path,
    state: Option<&str>,
    limit: usize,
) -> Result<Vec<QueueTicketBridgeListItem>> {
    let conn = open_queue_bridge_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT message_key, work_id, ticket_system, bridge_state, spilled_at, restored_at
        FROM queue_ticket_spills
        WHERE (?1 IS NULL OR bridge_state = ?1)
        ORDER BY updated_at DESC, spilled_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![state, limit as i64], |row| {
        Ok(QueueTicketBridgeRecord {
            message_key: row.get(0)?,
            work_id: row.get(1)?,
            ticket_system: row.get(2)?,
            bridge_state: row.get(3)?,
            spilled_at: row.get(4)?,
            restored_at: row.get(5)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        let bridge = row?;
        let task = channels::load_queue_task(root, &bridge.message_key)?;
        let ticket = tickets::load_ticket_self_work_item(root, &bridge.work_id)?;
        items.push(QueueTicketBridgeListItem {
            message_key: bridge.message_key,
            work_id: bridge.work_id,
            ticket_system: bridge.ticket_system,
            bridge_state: bridge.bridge_state,
            spilled_at: bridge.spilled_at,
            restored_at: bridge.restored_at,
            task,
            ticket,
        });
    }
    Ok(items)
}

fn list_queue_spill_candidates(root: &Path, limit: usize) -> Result<Vec<QueueSpillCandidateView>> {
    let tasks = channels::list_queue_tasks(
        root,
        &["pending".to_string(), "blocked".to_string()],
        10_000,
    )?;
    let mut candidates = Vec::new();
    for task in tasks {
        if let Some(candidate) = score_queue_spill_candidate(root, task)? {
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|left, right| {
        right
            .candidate_score
            .cmp(&left.candidate_score)
            .then_with(|| left.priority.cmp(&right.priority))
            .then_with(|| left.title.cmp(&right.title))
    });
    candidates.truncate(limit);
    Ok(candidates)
}

fn score_queue_spill_candidate(
    root: &Path,
    task: channels::QueueTaskView,
) -> Result<Option<QueueSpillCandidateView>> {
    if let Some(existing) = load_queue_ticket_bridge(root, &task.message_key)? {
        if existing.bridge_state == "spilled" {
            return Ok(None);
        }
    }
    if matches!(task.route_status.as_str(), "handled" | "cancelled") {
        return Ok(None);
    }

    let mut score = 0i64;
    let mut reasons = Vec::new();

    match task.priority.as_str() {
        "low" => {
            score += 4;
            reasons.push("priority is low, so it can leave the hot queue first".to_string());
        }
        "normal" => {
            score += 2;
            reasons.push(
                "priority is normal and can be deferred if queue pressure is high".to_string(),
            );
        }
        "high" => {
            score -= 1;
            reasons.push(
                "priority is high, so spill only if higher-risk work must stay hot".to_string(),
            );
        }
        "urgent" => {
            reasons.push(
                "priority is urgent, so it should normally stay in the hot queue".to_string(),
            );
            return Ok(None);
        }
        _ => {
            score += 1;
            reasons.push("priority is not classified as urgent".to_string());
        }
    }

    match task.route_status.as_str() {
        "blocked" => {
            score += 5;
            reasons.push("task is already blocked, so moving it into internal ticket tracking reduces queue pressure without losing it".to_string());
        }
        "pending" => {
            score += 1;
        }
        _ => {}
    }

    if task.workspace_root.is_some() {
        score += 1;
        reasons.push(
            "workspace context is already attached, which makes later restoration safer"
                .to_string(),
        );
    }
    if task.suggested_skill.is_some() {
        score += 1;
        reasons.push(
            "task already names a suggested skill, so it can re-enter the loop cleanly later"
                .to_string(),
        );
    }
    if task.parent_message_key.is_some() {
        score -= 2;
        reasons.push(
            "task has a parent queue message, so spilling it may hide active continuity"
                .to_string(),
        );
    }

    if score <= 0 {
        return Ok(None);
    }

    let recommendation = if task.route_status == "blocked" {
        "strong spill candidate".to_string()
    } else if score >= 5 {
        "good spill candidate".to_string()
    } else {
        "spill only if pressure remains high".to_string()
    };

    Ok(Some(QueueSpillCandidateView {
        message_key: task.message_key,
        priority: task.priority,
        route_status: task.route_status,
        title: task.title,
        thread_key: task.thread_key,
        suggested_skill: task.suggested_skill,
        workspace_root: task.workspace_root,
        candidate_score: score,
        recommendation,
        reasons,
    }))
}

fn now_iso_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn ensure_edit_requested(args: &[String], value_flags: &[&str], bool_flags: &[&str]) -> Result<()> {
    let has_value_change = value_flags
        .iter()
        .any(|flag| find_flag_value(args, flag).is_some());
    let has_bool_change = bool_flags
        .iter()
        .any(|flag| args.iter().any(|arg| arg == flag));
    if has_value_change || has_bool_change {
        return Ok(());
    }
    anyhow::bail!("queue edit requires at least one field change")
}

fn default_thread_key(title: &str) -> String {
    let slug = title
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    let digest = stable_digest(title);
    if slug.is_empty() {
        format!("queue/task-{digest}")
    } else {
        format!("queue/{slug}-{digest}")
    }
}

fn stable_digest(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex = format!("{digest:x}");
    hex[..12].to_string()
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn collect_flag_values(args: &[String], flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == flag {
            if let Some(value) = args.get(index + 1) {
                values.push(value.clone());
                index += 2;
                continue;
            }
        }
        index += 1;
    }
    values
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("ctox-queue-test-{}-{}", label, std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn queue_task_can_spill_to_internal_ticket_and_restore() -> Result<()> {
        let root = temp_root("spill-restore");
        std::fs::create_dir_all(&root)?;

        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Investigate monitoring drift".to_string(),
                prompt: "Inspect Prometheus drift and report likely root cause.".to_string(),
                thread_key: "queue/monitoring-drift".to_string(),
                workspace_root: Some("/tmp/monitoring".to_string()),
                priority: "high".to_string(),
                suggested_skill: Some("reliability-ops".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;

        let spilled = spill_queue_task_to_ticket(
            &root,
            &task.message_key,
            DEFAULT_TICKET_SYSTEM,
            Some("queue pressure exceeded safe working set"),
            None,
            false,
        )?;
        assert_eq!(spilled.bridge_state, "spilled");
        assert_eq!(spilled.task.route_status, "blocked");
        let open_after_spill =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)?;
        assert_eq!(open_after_spill.len(), 1);
        assert_eq!(open_after_spill[0].route_status, "leased");
        assert_eq!(
            open_after_spill[0].lease_owner.as_deref(),
            Some(SPILL_RESTORE_LEASE_OWNER)
        );
        assert!(open_after_spill[0].title.starts_with("spill restore:"));
        assert_eq!(spilled.ticket.kind, "queue-overflow");
        assert_eq!(
            spilled.ticket.suggested_skill.as_deref(),
            Some("reliability-ops")
        );
        assert_eq!(
            spilled
                .ticket
                .metadata
                .get("queue_message_key")
                .and_then(serde_json::Value::as_str),
            Some(task.message_key.as_str())
        );

        let restored = restore_spilled_queue_task(
            &root,
            &task.message_key,
            Some("urgent"),
            Some("resume after ticket review"),
        )?;
        assert_eq!(restored.bridge_state, "restored");
        assert_eq!(restored.task.route_status, "leased");
        assert_eq!(
            restored.task.lease_owner.as_deref(),
            Some(SPILL_RESTORE_LEASE_OWNER)
        );
        assert!(restored
            .task
            .title
            .starts_with("restored queue rehydrate follow-up:"));
        assert_eq!(restored.task.priority, "urgent");
        assert_eq!(restored.ticket.state, "restored");
        let open_after_restore =
            channels::list_queue_tasks(&root, &["pending".to_string(), "leased".to_string()], 10)?;
        assert_eq!(open_after_restore.len(), 1);
        assert_eq!(open_after_restore[0].message_key, restored.task.message_key);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn spill_candidates_rank_blocked_and_lower_priority_tasks_first() -> Result<()> {
        let root = temp_root("spill-candidates");
        std::fs::create_dir_all(&root)?;

        let blocked = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Blocked low-priority audit".to_string(),
                prompt: "Review old audit findings.".to_string(),
                thread_key: "queue/audit".to_string(),
                workspace_root: Some("/tmp/audit".to_string()),
                priority: "low".to_string(),
                suggested_skill: Some("audit-review".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;
        let _ = channels::update_queue_task(
            &root,
            channels::QueueTaskUpdateRequest {
                message_key: blocked.message_key.clone(),
                route_status: Some("blocked".to_string()),
                status_note: Some("waiting for quieter window".to_string()),
                ..Default::default()
            },
        )?;

        let urgent = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Urgent prod incident".to_string(),
                prompt: "Handle production incident.".to_string(),
                thread_key: "queue/incident".to_string(),
                workspace_root: None,
                priority: "urgent".to_string(),
                suggested_skill: Some("incident-response".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;

        let candidates = list_queue_spill_candidates(&root, 10)?;
        assert_eq!(
            candidates.first().map(|item| item.message_key.as_str()),
            Some(blocked.message_key.as_str())
        );
        assert!(!candidates
            .iter()
            .any(|item| item.message_key == urgent.message_key));
        assert!(candidates
            .first()
            .map(|item| item
                .reasons
                .iter()
                .any(|reason| reason.contains("already blocked")))
            .unwrap_or(false));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn spills_list_returns_joined_queue_and_ticket_state() -> Result<()> {
        let root = temp_root("spills-list");
        std::fs::create_dir_all(&root)?;

        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Deferred documentation review".to_string(),
                prompt: "Review documentation backlog.".to_string(),
                thread_key: "queue/docs".to_string(),
                workspace_root: None,
                priority: "normal".to_string(),
                suggested_skill: Some("docs-review".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;
        let bridge = spill_queue_task_to_ticket(
            &root,
            &task.message_key,
            DEFAULT_TICKET_SYSTEM,
            None,
            None,
            false,
        )?;

        let spills = list_queue_ticket_bridges(&root, Some("spilled"), 10)?;
        assert_eq!(spills.len(), 1);
        assert_eq!(spills[0].message_key, task.message_key);
        assert_eq!(spills[0].work_id, bridge.work_id);
        assert_eq!(
            spills[0].ticket.as_ref().map(|item| item.kind.as_str()),
            Some("queue-overflow")
        );
        assert_eq!(
            spills[0]
                .task
                .as_ref()
                .map(|item| item.route_status.as_str()),
            Some("blocked")
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn queue_repair_releases_historical_plan_routes() -> Result<()> {
        let root = temp_root("queue-repair");
        std::fs::create_dir_all(&root)?;

        let created = plan::ingest_goal(
            &root,
            plan::PlanIngestRequest {
                title: "Repair stale queue route".to_string(),
                prompt: "- inspect runtime\n- verify route".to_string(),
                thread_key: Some("kunstmen-supervisor".to_string()),
                skill: Some("follow-up-orchestrator".to_string()),
                auto_advance: true,
                emit_now: true,
            },
        )?;
        let emitted = format!(
            "plan:system::{}::{}",
            created.goal.goal_id, created.steps[0].step_id
        );
        let conn = Connection::open(root.join("runtime/ctox.sqlite3"))?;
        conn.execute(
            "UPDATE communication_routing_state SET route_status = 'leased', lease_owner = 'test-reviewer', leased_at = ?2 WHERE message_key = ?1",
            params![emitted, chrono::Utc::now().to_rfc3339()],
        )?;
        conn.execute(
            "UPDATE planned_steps SET status = 'completed' WHERE step_id = ?1",
            params![created.steps[0].step_id.clone()],
        )?;
        drop(conn);

        let repaired = repair_queue_state(&root, true, false)?;
        assert_eq!(repaired.stale_plan_routes_repaired, 1);
        assert!(repaired.open_queue_preview.is_empty());
        assert!(repaired.agentic.is_none());

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn parse_queue_repair_action_line_supports_reprioritize() {
        let parsed = parse_queue_repair_action_line(
            "reprioritize queue:system::abc123 urgent :: founder mail must preempt stale review churn",
        )
        .expect("action should parse");
        assert_eq!(parsed.action, "reprioritize");
        assert_eq!(parsed.message_key, "queue:system::abc123");
        assert_eq!(parsed.priority.as_deref(), Some("urgent"));
        assert!(parsed.reason.contains("founder mail"));
    }

    #[test]
    fn apply_queue_repair_actions_updates_queue_state() -> Result<()> {
        let root = temp_root("queue-repair-actions");
        std::fs::create_dir_all(&root)?;

        let task = channels::create_queue_task(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Stale review rework".to_string(),
                prompt: "Cancel me".to_string(),
                thread_key: "kunstmen-supervisor".to_string(),
                workspace_root: None,
                priority: "normal".to_string(),
                suggested_skill: Some("queue-orchestrator".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;
        let actions = vec![QueueRepairActionView {
            action: "cancel".to_string(),
            message_key: task.message_key.clone(),
            priority: None,
            reason: "superseded by canonical supervisor task".to_string(),
        }];
        let applied = apply_queue_repair_actions(&root, &actions)?;
        assert_eq!(applied.len(), 1);
        let updated =
            channels::load_queue_task(&root, &task.message_key)?.expect("updated task missing");
        assert_eq!(updated.route_status, "cancelled");
        assert_eq!(
            updated.status_note.as_deref(),
            Some("superseded by canonical supervisor task")
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}
