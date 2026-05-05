// Origin: CTOX
// License: Apache-2.0

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

const USAGE: &str = "Usage:
  ctox harness-flow [--latest] [--message-key <key>] [--work-id <id>] [--width <n>] [--json]
  ctox harness-flow init
  ctox harness-flow events [--message-key <key>] [--work-id <id>] [--ticket-key <key>] [--limit <n>]

Renders a human-readable harness work flow: main work stays on the left spine,
while queue, context, review, ticket, knowledge, guard, and verification support
processes branch off at the point where they affect the work.";

#[derive(Debug, Clone, Serialize)]
pub struct HarnessFlow {
    pub schema_version: u32,
    pub source: FlowSource,
    pub ledger_events: Vec<HarnessFlowEvent>,
    pub blocks: Vec<MainBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowSource {
    pub message_key: Option<String>,
    pub work_id: Option<String>,
    pub source_kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainBlock {
    pub kind: FlowBlockKind,
    pub title: String,
    pub lines: Vec<String>,
    pub branches: Vec<SupportBranch>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowBlockKind {
    Task,
    Attempt,
    Finish,
    Empty,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupportBranch {
    pub kind: FlowBranchKind,
    pub title: String,
    pub lines: Vec<String>,
    pub returns_to_spine: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowBranchKind {
    QueuePickup,
    Context,
    Knowledge,
    Review,
    TicketBacklog,
    TicketSource,
    QueueReload,
    Guard,
    StateMachine,
    Verification,
    ProcessMining,
    HarnessLedger,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessFlowEvent {
    pub event_id: String,
    pub chain_key: String,
    pub event_kind: String,
    pub title: String,
    pub body_text: String,
    pub message_key: Option<String>,
    pub work_id: Option<String>,
    pub ticket_key: Option<String>,
    pub attempt_index: Option<i64>,
    pub metadata_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RecordHarnessFlowEventRequest<'a> {
    pub event_kind: &'a str,
    pub title: &'a str,
    pub body_text: &'a str,
    pub message_key: Option<&'a str>,
    pub work_id: Option<&'a str>,
    pub ticket_key: Option<&'a str>,
    pub attempt_index: Option<i64>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
struct MessageRow {
    message_key: String,
    channel: String,
    direction: String,
    thread_key: String,
    subject: String,
    preview: String,
    body_text: String,
    sender_display: String,
    observed_at: String,
}

#[derive(Debug, Clone)]
struct RoutingRow {
    route_status: String,
    lease_owner: Option<String>,
    leased_at: Option<String>,
    acked_at: Option<String>,
    last_error: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct SelfWorkRow {
    work_id: String,
    kind: String,
    title: String,
    body_text: String,
    state: String,
    metadata_json: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct FounderReviewApproval {
    approval_key: String,
    review_summary: String,
    body_sha256: String,
    reviewer: String,
    approved_at: String,
    sent_at: Option<String>,
}

#[derive(Debug, Clone)]
struct CoreProofRow {
    accepted: bool,
    entity_type: String,
    from_state: String,
    to_state: String,
    core_event: String,
    violation_codes_json: String,
    proof_id: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct StateViolationRow {
    severity: String,
    violation_code: String,
    message: String,
    detected_at: String,
}

pub fn handle_harness_flow_command(root: &Path, args: &[String]) -> Result<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{USAGE}");
        return Ok(());
    }
    match args.first().map(String::as_str) {
        Some("init") => {
            let conn = open_event_connection(root)?;
            ensure_event_schema(&conn)?;
            println!("harness flow event ledger ready");
            return Ok(());
        }
        Some("events") => {
            let message_key = parse_string_flag(&args[1..], "--message-key");
            let work_id = parse_string_flag(&args[1..], "--work-id");
            let ticket_key = parse_string_flag(&args[1..], "--ticket-key");
            let limit = parse_usize_flag(&args[1..], "--limit", 50).min(500);
            let events = load_flow_events(root, message_key, work_id, ticket_key, limit)?;
            println!("{}", serde_json::to_string_pretty(&events)?);
            return Ok(());
        }
        _ => {}
    }

    let width = parse_usize_flag(args, "--width", 118).clamp(92, 180);
    let message_key = parse_string_flag(args, "--message-key");
    let work_id = parse_string_flag(args, "--work-id");
    let flow = build_flow(root, message_key, work_id)?;
    if args
        .iter()
        .any(|arg| arg == "--json" || arg == "--format=json")
    {
        println!("{}", serde_json::to_string_pretty(&flow)?);
    } else {
        println!("{}", render_ascii(&flow, width));
    }
    Ok(())
}

pub fn render_latest_ascii(root: &Path, width: usize) -> Result<String> {
    let flow = build_flow(root, None, None)?;
    Ok(render_ascii(&flow, width.clamp(92, 180)))
}

#[allow(dead_code)]
pub fn render_selected_ascii(
    root: &Path,
    message_key: Option<&str>,
    work_id: Option<&str>,
    width: usize,
) -> Result<String> {
    let flow = build_flow(root, message_key, work_id)?;
    Ok(render_ascii(&flow, width.clamp(92, 180)))
}

#[allow(dead_code)]
pub fn load_latest_flow(root: &Path) -> Result<HarnessFlow> {
    build_flow(root, None, None)
}

#[allow(dead_code)]
pub fn load_selected_flow(
    root: &Path,
    message_key: Option<&str>,
    work_id: Option<&str>,
) -> Result<HarnessFlow> {
    build_flow(root, message_key, work_id)
}

#[allow(dead_code)]
pub fn init_event_ledger(root: &Path) -> Result<()> {
    let conn = open_event_connection(root)?;
    ensure_event_schema(&conn)
}

#[allow(dead_code)]
pub fn record_harness_flow_event(
    root: &Path,
    request: RecordHarnessFlowEventRequest<'_>,
) -> Result<HarnessFlowEvent> {
    let conn = open_event_connection(root)?;
    ensure_event_schema(&conn)?;
    let created_at = Utc::now().to_rfc3339();
    let chain_key = chain_key(request.message_key, request.work_id, request.ticket_key);
    let metadata_json = serde_json::to_string(&request.metadata)?;
    let event_id = event_id(
        &chain_key,
        request.event_kind,
        request.title,
        request.body_text,
        &created_at,
    );
    conn.execute(
        "INSERT INTO ctox_harness_flow_events (
            event_id, chain_key, event_kind, title, body_text,
            message_key, work_id, ticket_key, attempt_index, metadata_json, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            event_id,
            chain_key,
            request.event_kind,
            request.title,
            request.body_text,
            request.message_key,
            request.work_id,
            request.ticket_key,
            request.attempt_index,
            metadata_json,
            created_at,
        ],
    )?;
    Ok(HarnessFlowEvent {
        event_id,
        chain_key,
        event_kind: request.event_kind.to_string(),
        title: request.title.to_string(),
        body_text: request.body_text.to_string(),
        message_key: request.message_key.map(ToOwned::to_owned),
        work_id: request.work_id.map(ToOwned::to_owned),
        ticket_key: request.ticket_key.map(ToOwned::to_owned),
        attempt_index: request.attempt_index,
        metadata_json,
        created_at,
    })
}

pub fn record_harness_flow_event_lossy(root: &Path, request: RecordHarnessFlowEventRequest<'_>) {
    let _ = record_harness_flow_event(root, request);
}

fn build_flow(
    root: &Path,
    message_key: Option<&str>,
    work_id: Option<&str>,
) -> Result<HarnessFlow> {
    let db_path = root.join("runtime").join("ctox.sqlite3");
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open {}", db_path.display()))?;

    let seed_work = match work_id {
        Some(id) => load_self_work(&conn, id)?,
        None => None,
    };
    let seed_message = match message_key {
        Some(key) => load_message(&conn, key)?,
        None => {
            if let Some(work) = seed_work.as_ref() {
                parent_message_key(&work.metadata_json)
                    .as_deref()
                    .and_then(|key| load_message(&conn, key).ok().flatten())
            } else {
                latest_message(&conn)?
            }
        }
    };

    let Some(message) = seed_message else {
        let mut blocks = Vec::new();
        blocks.push(MainBlock {
            kind: FlowBlockKind::Empty,
            title: "NO FLOW SOURCE FOUND".to_string(),
            lines: vec![
                "No communication message or self-work item matched the request.".to_string(),
                "Try --message-key <key> or --work-id <id>.".to_string(),
            ],
            branches: Vec::new(),
        });
        return Ok(HarnessFlow {
            schema_version: 1,
            source: FlowSource {
                message_key: message_key.map(ToOwned::to_owned),
                work_id: work_id.map(ToOwned::to_owned),
                source_kind: "empty".to_string(),
            },
            ledger_events: Vec::new(),
            blocks,
        });
    };

    let routing = load_routing(&conn, &message.message_key)?;
    let related_work = load_related_self_work(&conn, &message.message_key)?;
    let work_for_attempt_2 = seed_work.or_else(|| related_work.first().cloned());
    let reload_message = work_for_attempt_2.as_ref().and_then(|work| {
        load_queue_message_for_work(&conn, &work.work_id)
            .ok()
            .flatten()
    });
    let review_approval = load_founder_review_approval(&conn, &message.message_key)?;
    let proofs = load_core_proofs_for_key(&conn, &message.message_key)?;
    let violations = load_state_violations_for_key(&conn, &message.message_key)?;
    let ledger_events = load_flow_events(
        root,
        Some(&message.message_key),
        work_for_attempt_2
            .as_ref()
            .map(|work| work.work_id.as_str()),
        None,
        12,
    )
    .unwrap_or_default();

    let mut blocks = Vec::new();
    blocks.push(MainBlock {
        kind: FlowBlockKind::Task,
        title: "TASK".to_string(),
        lines: task_lines(&message),
        branches: {
            let mut branches = vec![
                queue_pickup_branch(routing.as_ref()),
                context_branch(&conn)?,
                knowledge_branch(&conn)?,
            ];
            if let Some(branch) = ledger_branch(&ledger_events) {
                branches.push(branch);
            }
            branches
        },
    });

    blocks.push(MainBlock {
        kind: FlowBlockKind::Attempt,
        title: "ATTEMPT 1".to_string(),
        lines: attempt_lines("CTOX works on the first answer or slice.", &message, None),
        branches: attempt_one_branches(
            &message,
            related_work.as_slice(),
            review_approval.as_ref(),
            violations.as_slice(),
        ),
    });

    if let Some(work) = work_for_attempt_2.as_ref() {
        let mut branches = Vec::new();
        branches.push(ticket_pickup_branch(work, reload_message.as_ref()));
        if let Some(reload) = reload_message.as_ref() {
            branches.push(queue_reload_branch(
                reload,
                load_routing(&conn, &reload.message_key)?.as_ref(),
            ));
        }
        blocks.push(MainBlock {
            kind: FlowBlockKind::Attempt,
            title: "ATTEMPT 2 / REWORK".to_string(),
            lines: attempt_lines(
                "CTOX resumes from durable rework with the prior review attached.",
                reload_message.as_ref().unwrap_or(&message),
                Some(work),
            ),
            branches,
        });
    }

    blocks.push(MainBlock {
        kind: FlowBlockKind::Finish,
        title: "FINISH / CURRENT STATE".to_string(),
        lines: finish_lines(
            routing.as_ref(),
            work_for_attempt_2.as_ref(),
            review_approval.as_ref(),
        ),
        branches: vec![
            state_machine_branch(&conn)?,
            guard_branch(proofs.as_slice(), violations.as_slice()),
            verification_branch(&conn, work_for_attempt_2.as_ref())?,
            process_mining_branch(&conn)?,
        ],
    });

    Ok(HarnessFlow {
        schema_version: 1,
        source: FlowSource {
            message_key: Some(message.message_key),
            work_id: work_for_attempt_2.map(|work| work.work_id),
            source_kind: if work_id.is_some() {
                "work".to_string()
            } else {
                "message".to_string()
            },
        },
        ledger_events,
        blocks,
    })
}

fn task_lines(message: &MessageRow) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "{} from {}",
        sentence_case(&message.direction),
        non_empty(&message.sender_display, "unknown sender")
    ));
    lines.push(format!(
        "Subject: {}",
        clip(non_empty(&message.subject, "(no subject)"), 82)
    ));
    let preview = first_non_empty(&[&message.preview, &message.body_text]).unwrap_or("");
    if !preview.trim().is_empty() {
        lines.push(format!("What CTOX has to handle: {}", clip(preview, 82)));
    }
    lines.push(format!(
        "Source: {} · thread {} · observed {}",
        message.channel,
        clip(&message.thread_key, 38),
        short_time(&message.observed_at)
    ));
    lines
}

fn queue_pickup_branch(routing: Option<&RoutingRow>) -> SupportBranch {
    let lines = match routing {
        Some(row) => {
            let mut lines = Vec::new();
            lines.push(format!("Current queue state: {}", row.route_status));
            if let Some(owner) = row.lease_owner.as_deref().filter(|s| !s.is_empty()) {
                lines.push(format!("Leased by: {}", owner));
            }
            if let Some(leased_at) = row.leased_at.as_deref().filter(|s| !s.is_empty()) {
                lines.push(format!("Lease time: {}", short_time(leased_at)));
            }
            if let Some(acked_at) = row.acked_at.as_deref().filter(|s| !s.is_empty()) {
                lines.push(format!("Acknowledged: {}", short_time(acked_at)));
            }
            if let Some(error) = row.last_error.as_deref().filter(|s| !s.is_empty()) {
                lines.push(format!("Queue error: {}", clip(error, 72)));
            }
            lines.push(format!(
                "Last queue update: {}",
                short_time(&row.updated_at)
            ));
            lines
        }
        None => vec!["No routing row found for this source yet.".to_string()],
    };
    SupportBranch {
        kind: FlowBranchKind::QueuePickup,
        title: "QUEUE PICKUP".to_string(),
        lines,
        returns_to_spine: true,
    }
}

fn context_branch(conn: &Connection) -> Result<SupportBranch> {
    let mission = optional_string(
        conn,
        "SELECT mission FROM mission_states ORDER BY last_synced_at DESC LIMIT 1",
        [],
    )?;
    let docs = optional_count(conn, "SELECT COUNT(*) FROM continuity_documents")?;
    let commits = optional_count(conn, "SELECT COUNT(*) FROM continuity_commits")?;
    let mut lines = Vec::new();
    if let Some(mission) = mission.filter(|s| !s.trim().is_empty()) {
        lines.push(format!("Current mission: {}", clip(&mission, 76)));
    } else {
        lines.push("No active mission text found.".to_string());
    }
    lines.push(format!("Continuity docs: {} · commits: {}", docs, commits));
    lines.push("Purpose: keep the worker on the current task context.".to_string());
    Ok(SupportBranch {
        kind: FlowBranchKind::Context,
        title: "CONTEXT".to_string(),
        lines,
        returns_to_spine: true,
    })
}

fn knowledge_branch(conn: &Connection) -> Result<SupportBranch> {
    let loads = optional_count(conn, "SELECT COUNT(*) FROM ticket_knowledge_loads")?;
    let entries = optional_count(conn, "SELECT COUNT(*) FROM ticket_knowledge_entries")?;
    let skills = optional_count(conn, "SELECT COUNT(*) FROM knowledge_main_skills")?;
    let mut lines = Vec::new();
    lines.push(format!(
        "Ticket fact loads: {} · fact/context entries: {} · main skills: {}",
        loads, entries, skills
    ));
    let latest = optional_string(
        conn,
        "SELECT title FROM ticket_knowledge_entries ORDER BY updated_at DESC LIMIT 1",
        [],
    )?;
    if let Some(title) = latest.filter(|s| !s.trim().is_empty()) {
        lines.push(format!(
            "Latest ticket fact/context entry: {}",
            clip(&title, 72)
        ));
    } else {
        lines.push("No task-specific knowledge capture observed yet.".to_string());
    }
    Ok(SupportBranch {
        kind: FlowBranchKind::Knowledge,
        title: "KNOWLEDGE".to_string(),
        lines,
        returns_to_spine: true,
    })
}

fn ledger_branch(events: &[HarnessFlowEvent]) -> Option<SupportBranch> {
    if events.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    lines.push(format!(
        "Durable flow events linked to this chain: {}",
        events.len()
    ));
    for event in events.iter().take(4) {
        let suffix = if event.body_text.trim().is_empty() {
            String::new()
        } else {
            format!(": {}", clip(&event.body_text, 58))
        };
        lines.push(format!(
            "{} · {}{}",
            short_time(&event.created_at),
            clip(&event.title, 42),
            suffix
        ));
    }
    Some(SupportBranch {
        kind: FlowBranchKind::HarnessLedger,
        title: "HARNESS LEDGER".to_string(),
        lines,
        returns_to_spine: true,
    })
}

fn attempt_lines(intro: &str, message: &MessageRow, work: Option<&SelfWorkRow>) -> Vec<String> {
    let mut lines = vec![intro.to_string()];
    if let Some(work) = work {
        lines.push(format!(
            "Picked up: {} ({})",
            clip(&work.title, 70),
            work.kind
        ));
        lines.push(format!(
            "Backlog state: {} · updated {}",
            work.state,
            short_time(&work.updated_at)
        ));
        if !work.body_text.trim().is_empty() {
            lines.push(format!("Needed work: {}", clip(&work.body_text, 82)));
        }
    } else {
        lines.push(format!(
            "Input: {}",
            clip(
                first_non_empty(&[&message.preview, &message.body_text]).unwrap_or(""),
                82
            )
        ));
    }
    lines.push(
        "Work metrics: not instrumented yet (files/line deltas need a turn diff ledger)."
            .to_string(),
    );
    lines
}

fn attempt_one_branches(
    message: &MessageRow,
    related_work: &[SelfWorkRow],
    approval: Option<&FounderReviewApproval>,
    violations: &[StateViolationRow],
) -> Vec<SupportBranch> {
    let mut branches = Vec::new();
    if let Some(approval) = approval {
        branches.push(review_pass_branch(approval));
    } else if !related_work.is_empty() {
        branches.push(review_rework_branch(related_work));
    } else if !violations.is_empty() {
        branches.push(review_blocked_branch(violations));
    } else {
        branches.push(SupportBranch {
            kind: FlowBranchKind::Review,
            title: "REVIEW".to_string(),
            lines: vec![
                "No persisted review result found for this source.".to_string(),
                "If a review happened, the flow needs that outcome captured durably.".to_string(),
            ],
            returns_to_spine: true,
        });
    }

    if !related_work.is_empty() {
        branches.push(ticket_sink_branch(message, related_work));
    }
    branches
}

fn review_pass_branch(approval: &FounderReviewApproval) -> SupportBranch {
    let mut lines = Vec::new();
    lines.push("Result: send allowed.".to_string());
    lines.push(format!(
        "Review summary: {}",
        clip(&approval.review_summary, 76)
    ));
    lines.push(format!(
        "Reviewer: {} · approved {}",
        approval.reviewer,
        short_time(&approval.approved_at)
    ));
    lines.push(format!(
        "Approved body hash: {}",
        clip(&approval.body_sha256, 28)
    ));
    if let Some(sent_at) = approval.sent_at.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("Sent after review: {}", short_time(sent_at)));
    }
    SupportBranch {
        kind: FlowBranchKind::Review,
        title: "REVIEW".to_string(),
        lines,
        returns_to_spine: true,
    }
}

fn review_rework_branch(related_work: &[SelfWorkRow]) -> SupportBranch {
    let first = &related_work[0];
    SupportBranch {
        kind: FlowBranchKind::Review,
        title: "REVIEW".to_string(),
        lines: vec![
            "Result: not finished; durable rework exists.".to_string(),
            "Communication rework is capped: at most two substantive reworks, then one wording-only rewrite.".to_string(),
            "The next review must compare against the previous review context, not start from a blank slate.".to_string(),
            format!("Rework item: {}", clip(&first.title, 76)),
            format!("Reason/work requested: {}", clip(&first.body_text, 76)),
        ],
        returns_to_spine: false,
    }
}

fn review_blocked_branch(violations: &[StateViolationRow]) -> SupportBranch {
    let mut lines = Vec::new();
    lines.push("Result: blocked by harness guard.".to_string());
    let mut seen_codes = Vec::<&str>::new();
    for violation in violations {
        if seen_codes
            .iter()
            .any(|code| *code == violation.violation_code)
        {
            continue;
        }
        seen_codes.push(&violation.violation_code);
        if seen_codes.len() > 3 {
            break;
        }
        lines.push(format!(
            "{}: {}",
            violation.violation_code,
            clip(&violation.message, 68)
        ));
    }
    SupportBranch {
        kind: FlowBranchKind::Review,
        title: "REVIEW / SEND BLOCK".to_string(),
        lines,
        returns_to_spine: false,
    }
}

fn ticket_sink_branch(message: &MessageRow, related_work: &[SelfWorkRow]) -> SupportBranch {
    let mut lines = Vec::new();
    lines.push(format!(
        "Original source: {}",
        clip(&message.message_key, 62)
    ));
    for work in related_work.iter().take(3) {
        lines.push(format!(
            "Created: {} · {}",
            clip(&work.work_id, 18),
            clip(&work.title, 52)
        ));
        lines.push(format!("State: {} · kind: {}", work.state, work.kind));
    }
    SupportBranch {
        kind: FlowBranchKind::TicketBacklog,
        title: "TICKET BACKLOG".to_string(),
        lines,
        returns_to_spine: false,
    }
}

fn ticket_pickup_branch(work: &SelfWorkRow, reload_message: Option<&MessageRow>) -> SupportBranch {
    let mut lines = Vec::new();
    lines.push(format!("Picked up work item: {}", clip(&work.work_id, 48)));
    lines.push(format!("Title: {}", clip(&work.title, 74)));
    lines.push(format!(
        "State: {} · created {}",
        work.state,
        short_time(&work.created_at)
    ));
    if let Some(message) = reload_message {
        lines.push(format!(
            "Re-entered queue as: {}",
            clip(&message.message_key, 58)
        ));
    } else {
        lines.push("No queue reload message found for this work item.".to_string());
    }
    SupportBranch {
        kind: FlowBranchKind::TicketSource,
        title: "SOURCE FROM TICKET BACKLOG".to_string(),
        lines,
        returns_to_spine: true,
    }
}

fn queue_reload_branch(message: &MessageRow, routing: Option<&RoutingRow>) -> SupportBranch {
    let mut lines = Vec::new();
    lines.push(format!("Queue source: {}", clip(&message.message_key, 62)));
    lines.push(format!("Thread: {}", clip(&message.thread_key, 70)));
    if let Some(routing) = routing {
        lines.push(format!("Reload status: {}", routing.route_status));
    }
    lines.push("Effect: this backlog item can become the next worker job.".to_string());
    SupportBranch {
        kind: FlowBranchKind::QueueReload,
        title: "QUEUE RELOAD".to_string(),
        lines,
        returns_to_spine: true,
    }
}

fn finish_lines(
    routing: Option<&RoutingRow>,
    work: Option<&SelfWorkRow>,
    approval: Option<&FounderReviewApproval>,
) -> Vec<String> {
    let mut lines = Vec::new();
    match routing {
        Some(row) if row.route_status == "handled" => {
            lines.push("Original queue job is handled.".to_string())
        }
        Some(row) => lines.push(format!("Original queue state: {}", row.route_status)),
        None => lines.push("Original queue state: unknown".to_string()),
    }
    if let Some(work) = work {
        lines.push(format!(
            "Backlog item {} is currently {}.",
            clip(&work.work_id, 24),
            work.state
        ));
    }
    if let Some(approval) = approval {
        lines.push(format!(
            "Latest review approval: {}",
            clip(&approval.approval_key, 54)
        ));
    }
    if work.is_none() && approval.is_none() {
        lines.push("No ticket/self-work close or review approval is linked yet.".to_string());
    }
    lines
}

fn guard_branch(proofs: &[CoreProofRow], violations: &[StateViolationRow]) -> SupportBranch {
    let mut lines = Vec::new();
    if proofs.is_empty() && violations.is_empty() {
        lines.push("No core transition proof or violation found for this source.".to_string());
    }
    for proof in proofs.iter().take(3) {
        lines.push(format!(
            "{} {} -> {} ({})",
            if proof.accepted {
                "Accepted:"
            } else {
                "Rejected:"
            },
            proof.from_state,
            proof.to_state,
            proof.core_event
        ));
        lines.push(format!(
            "{} · {} · proof {}",
            proof.entity_type,
            short_time(&proof.updated_at),
            clip(&proof.proof_id, 24)
        ));
        if !proof.accepted {
            lines.push(format!(
                "Violations: {}",
                clip(&proof.violation_codes_json, 70)
            ));
        }
    }
    let mut seen_codes = Vec::<&str>::new();
    for violation in violations.iter() {
        if seen_codes
            .iter()
            .any(|code| *code == violation.violation_code)
        {
            continue;
        }
        seen_codes.push(&violation.violation_code);
        if seen_codes.len() > 3 {
            break;
        }
        lines.push(format!(
            "{} violation: {}",
            violation.severity,
            clip(&violation.violation_code, 58)
        ));
        lines.push(format!("Detected: {}", short_time(&violation.detected_at)));
    }
    SupportBranch {
        kind: FlowBranchKind::Guard,
        title: "SEND / CLOSE GUARD".to_string(),
        lines,
        returns_to_spine: true,
    }
}

fn state_machine_branch(conn: &Connection) -> Result<SupportBranch> {
    let open_rework = optional_count(
        conn,
        "SELECT COUNT(*) FROM ticket_self_work_items
         WHERE state IN ('open', 'assigned', 'in_progress')
           AND kind LIKE '%rework%'",
    )?;
    let review_checkpoint_blocks = optional_count(
        conn,
        "SELECT COUNT(*) FROM ctox_core_transition_proofs
         WHERE accepted = 0
           AND violation_codes_json LIKE '%review_checkpoint%'",
    )?;
    let outcome_blocks = optional_count(
        conn,
        "SELECT COUNT(*) FROM ctox_core_transition_proofs
         WHERE accepted = 0
           AND (
             violation_codes_json LIKE '%WP-Outcome-Missing%'
             OR violation_codes_json LIKE '%WP-Outcome-Wrong-State%'
           )",
    )?;
    let accepted_spawns = optional_count(
        conn,
        "SELECT COUNT(*) FROM ctox_core_spawn_edges WHERE accepted = 1",
    )?;
    let rejected_spawns = optional_count(
        conn,
        "SELECT COUNT(*) FROM ctox_core_spawn_edges WHERE accepted = 0",
    )?;
    let process_blocks = optional_count(
        conn,
        "SELECT COUNT(*) FROM ctox_pm_state_violations
         WHERE violation_code LIKE '%review%'
            OR violation_code LIKE '%rewrite%'
            OR violation_code LIKE '%rework%'
            OR violation_code LIKE '%outcome%'
            OR violation_code LIKE '%spawn%'",
    )?;
    Ok(SupportBranch {
        kind: FlowBranchKind::StateMachine,
        title: "HARNESS STATE MACHINE".to_string(),
        lines: vec![
            "Review Gate is a checkpoint: it gives feedback to the same main work item; it must not spawn review-work cascades.".to_string(),
            "Terminal work needs an outcome witness: text like sent/done is not evidence without the durable artifact.".to_string(),
            "Task spawning is allowed only through modeled parent -> child edges with checkpoint and budget discipline.".to_string(),
            "If the kernel rejects review, outcome, or spawn evidence, the agent resumes the original work and creates the missing artifact itself.".to_string(),
            format!("Open rework: {open_rework} · review checkpoint blocks: {review_checkpoint_blocks} · outcome blocks: {outcome_blocks}"),
            format!("Spawn edges accepted: {accepted_spawns} · rejected: {rejected_spawns} · process blocks: {process_blocks}"),
        ],
        returns_to_spine: true,
    })
}

fn verification_branch(conn: &Connection, work: Option<&SelfWorkRow>) -> Result<SupportBranch> {
    let count = optional_count(conn, "SELECT COUNT(*) FROM verification_runs")?;
    let mut lines = vec![format!("Verification runs in runtime: {}", count)];
    if let Some(work) = work {
        let matching = optional_count_with_param(
            conn,
            "SELECT COUNT(*) FROM verification_runs WHERE goal LIKE '%' || ?1 || '%'",
            &work.work_id,
        )?;
        lines.push(format!("Runs mentioning this work item: {}", matching));
    }
    let ticket_verifications = optional_count(conn, "SELECT COUNT(*) FROM ticket_verifications")?;
    lines.push(format!(
        "Ticket verification records: {}",
        ticket_verifications
    ));
    Ok(SupportBranch {
        kind: FlowBranchKind::Verification,
        title: "VERIFICATION".to_string(),
        lines,
        returns_to_spine: true,
    })
}

fn process_mining_branch(conn: &Connection) -> Result<SupportBranch> {
    let total_events = optional_count(conn, "SELECT COUNT(*) FROM ctox_process_events")?;
    let sqlite_access_events = optional_count(
        conn,
        "SELECT COUNT(*) FROM ctox_process_events WHERE case_id LIKE 'sqlite-access:%'",
    )?;
    Ok(SupportBranch {
        kind: FlowBranchKind::ProcessMining,
        title: "PROCESS MINING".to_string(),
        lines: vec![
            "Records compact command/state evidence, not full message bodies.".to_string(),
            "SQLite READ events are off by default; write/transition evidence remains active."
                .to_string(),
            "sqlite-access debug data is pruned as a sliding window, not kept forever.".to_string(),
            "Manual cleanup: ctox process-mining prune --sqlite-access-window 200000".to_string(),
            format!("Current events: {total_events} · sqlite-access: {sqlite_access_events}"),
        ],
        returns_to_spine: true,
    })
}

fn render_ascii(flow: &HarnessFlow, width: usize) -> String {
    let main_width = (width * 54 / 100).clamp(50, 82);
    let branch_width = width.saturating_sub(main_width + 8).clamp(34, 86);
    let mut out = String::new();

    for (index, block) in flow.blocks.iter().enumerate() {
        render_box(&mut out, "", main_width, &block.title, &block.lines);
        for branch in &block.branches {
            let stem_pad = " ".repeat(main_width / 2);
            out.push_str(&format!("{stem_pad}│\n"));
            render_branch_box(
                &mut out,
                &stem_pad,
                branch_width,
                &branch.title,
                &branch.lines,
            );
            if branch.returns_to_spine {
                out.push_str(&format!("{stem_pad}│\n"));
            }
        }
        if index + 1 < flow.blocks.len() {
            out.push_str(&format!(
                "{}│\n{}▼\n",
                " ".repeat(main_width / 2),
                " ".repeat(main_width / 2)
            ));
        }
    }

    out.trim_end().to_string()
}

fn render_box(out: &mut String, prefix: &str, width: usize, title: &str, lines: &[String]) {
    let inner = width.saturating_sub(2);
    out.push_str(prefix);
    out.push('┌');
    out.push_str(&"─".repeat(inner));
    out.push_str("┐\n");
    render_box_line(out, prefix, inner, title);
    for line in lines {
        for wrapped in wrap_line(line, inner.saturating_sub(2)) {
            render_box_line(out, prefix, inner, &format!("  {wrapped}"));
        }
    }
    out.push_str(prefix);
    out.push('└');
    out.push_str(&"─".repeat(inner));
    out.push_str("┘\n");
}

fn render_branch_box(
    out: &mut String,
    stem_pad: &str,
    width: usize,
    title: &str,
    lines: &[String],
) {
    let mut rendered = String::new();
    render_box(&mut rendered, "", width, title, lines);
    for (idx, line) in rendered.lines().enumerate() {
        out.push_str(stem_pad);
        if idx == 0 {
            out.push_str("├──►");
        } else {
            out.push_str("│   ");
        }
        out.push_str(line);
        out.push('\n');
    }
}

fn render_box_line(out: &mut String, prefix: &str, inner: usize, text: &str) {
    let clipped = clip(text, inner);
    out.push_str(prefix);
    out.push('│');
    out.push_str(&clipped);
    out.push_str(&" ".repeat(inner.saturating_sub(clipped.chars().count())));
    out.push_str("│\n");
}

fn wrap_line(text: &str, width: usize) -> Vec<String> {
    if text.chars().count() <= width {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let next_len =
            current.chars().count() + if current.is_empty() { 0 } else { 1 } + word.chars().count();
        if next_len > width && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        vec![clip(text, width)]
    } else {
        lines
    }
}

fn load_message(conn: &Connection, key: &str) -> Result<Option<MessageRow>> {
    query_message(
        conn,
        "SELECT message_key, channel, direction, thread_key, subject, preview, body_text,
                sender_display, observed_at
         FROM communication_messages WHERE message_key = ?1",
        params![key],
    )
}

fn latest_message(conn: &Connection) -> Result<Option<MessageRow>> {
    query_message(
        conn,
        "SELECT message_key, channel, direction, thread_key, subject, preview, body_text,
                sender_display, observed_at
         FROM communication_messages ORDER BY observed_at DESC LIMIT 1",
        [],
    )
}

fn query_message<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Option<MessageRow>> {
    Ok(conn
        .query_row(sql, params, |row| {
            Ok(MessageRow {
                message_key: row.get(0)?,
                channel: row.get(1)?,
                direction: row.get(2)?,
                thread_key: row.get(3)?,
                subject: row.get(4)?,
                preview: row.get(5)?,
                body_text: row.get(6)?,
                sender_display: row.get(7)?,
                observed_at: row.get(8)?,
            })
        })
        .ok())
}

fn load_routing(conn: &Connection, key: &str) -> Result<Option<RoutingRow>> {
    Ok(conn
        .query_row(
            "SELECT route_status, lease_owner, leased_at, acked_at, last_error, updated_at
             FROM communication_routing_state WHERE message_key = ?1",
            params![key],
            |row| {
                Ok(RoutingRow {
                    route_status: row.get(0)?,
                    lease_owner: row.get(1)?,
                    leased_at: row.get(2)?,
                    acked_at: row.get(3)?,
                    last_error: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .ok())
}

fn load_self_work(conn: &Connection, work_id: &str) -> Result<Option<SelfWorkRow>> {
    query_self_work(
        conn,
        "SELECT work_id, kind, title, body_text, state, metadata_json, created_at, updated_at
         FROM ticket_self_work_items WHERE work_id = ?1",
        params![work_id],
    )
}

fn load_related_self_work(conn: &Connection, message_key: &str) -> Result<Vec<SelfWorkRow>> {
    let mut stmt = match conn.prepare(
        "SELECT work_id, kind, title, body_text, state, metadata_json, created_at, updated_at
         FROM ticket_self_work_items
         WHERE json_extract(metadata_json, '$.parent_message_key') = ?1
            OR json_extract(metadata_json, '$.inbound_message_key') = ?1
            OR metadata_json LIKE '%' || ?1 || '%'
         ORDER BY created_at ASC LIMIT 8",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Ok(Vec::new()),
    };
    let rows = stmt.query_map(params![message_key], map_self_work)?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

fn query_self_work<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Option<SelfWorkRow>> {
    Ok(conn.query_row(sql, params, map_self_work).ok())
}

fn map_self_work(row: &rusqlite::Row<'_>) -> rusqlite::Result<SelfWorkRow> {
    Ok(SelfWorkRow {
        work_id: row.get(0)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        body_text: row.get(3)?,
        state: row.get(4)?,
        metadata_json: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn load_queue_message_for_work(conn: &Connection, work_id: &str) -> Result<Option<MessageRow>> {
    query_message(
        conn,
        "SELECT message_key, channel, direction, thread_key, subject, preview, body_text,
                sender_display, observed_at
         FROM communication_messages
         WHERE json_extract(metadata_json, '$.ticket_self_work_id') = ?1
            OR metadata_json LIKE '%' || ?1 || '%'
         ORDER BY observed_at ASC LIMIT 1",
        params![work_id],
    )
}

fn load_founder_review_approval(
    conn: &Connection,
    key: &str,
) -> Result<Option<FounderReviewApproval>> {
    Ok(conn
        .query_row(
            "SELECT approval_key, review_summary, body_sha256, reviewer, approved_at, sent_at
             FROM communication_founder_reply_reviews
             WHERE inbound_message_key = ?1
             ORDER BY approved_at DESC LIMIT 1",
            params![key],
            |row| {
                Ok(FounderReviewApproval {
                    approval_key: row.get(0)?,
                    review_summary: row.get(1)?,
                    body_sha256: row.get(2)?,
                    reviewer: row.get(3)?,
                    approved_at: row.get(4)?,
                    sent_at: row.get(5)?,
                })
            },
        )
        .ok())
}

fn load_core_proofs_for_key(conn: &Connection, key: &str) -> Result<Vec<CoreProofRow>> {
    let mut stmt = match conn.prepare(
        "SELECT accepted, entity_type, from_state, to_state, core_event,
                violation_codes_json, proof_id, updated_at
         FROM ctox_core_transition_proofs
         WHERE entity_id LIKE '%' || ?1 || '%'
         ORDER BY updated_at DESC LIMIT 8",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Ok(Vec::new()),
    };
    let rows = stmt.query_map(params![key], |row| {
        Ok(CoreProofRow {
            accepted: row.get::<_, i64>(0)? != 0,
            entity_type: row.get(1)?,
            from_state: row.get(2)?,
            to_state: row.get(3)?,
            core_event: row.get(4)?,
            violation_codes_json: row.get(5)?,
            proof_id: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

fn load_state_violations_for_key(conn: &Connection, key: &str) -> Result<Vec<StateViolationRow>> {
    let mut stmt = match conn.prepare(
        "SELECT severity, violation_code, message, detected_at
         FROM ctox_pm_state_violations
         WHERE case_id LIKE '%' || ?1 || '%'
            OR evidence_json LIKE '%' || ?1 || '%'
         ORDER BY detected_at DESC LIMIT 8",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Ok(Vec::new()),
    };
    let rows = stmt.query_map(params![key], |row| {
        Ok(StateViolationRow {
            severity: row.get(0)?,
            violation_code: row.get(1)?,
            message: row.get(2)?,
            detected_at: row.get(3)?,
        })
    })?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

fn open_event_connection(root: &Path) -> Result<Connection> {
    let db_path = root.join("runtime").join("ctox.sqlite3");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Connection::open(&db_path).with_context(|| format!("failed to open {}", db_path.display()))
}

fn ensure_event_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ctox_harness_flow_events (
            event_id TEXT PRIMARY KEY,
            chain_key TEXT NOT NULL,
            event_kind TEXT NOT NULL,
            title TEXT NOT NULL,
            body_text TEXT NOT NULL DEFAULT '',
            message_key TEXT,
            work_id TEXT,
            ticket_key TEXT,
            attempt_index INTEGER,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_ctox_harness_flow_events_chain
           ON ctox_harness_flow_events(chain_key, created_at ASC);
         CREATE INDEX IF NOT EXISTS idx_ctox_harness_flow_events_message
           ON ctox_harness_flow_events(message_key, created_at ASC);
         CREATE INDEX IF NOT EXISTS idx_ctox_harness_flow_events_work
           ON ctox_harness_flow_events(work_id, created_at ASC);
         CREATE INDEX IF NOT EXISTS idx_ctox_harness_flow_events_ticket
           ON ctox_harness_flow_events(ticket_key, created_at ASC);",
    )?;
    Ok(())
}

fn load_flow_events(
    root: &Path,
    message_key: Option<&str>,
    work_id: Option<&str>,
    ticket_key: Option<&str>,
    limit: usize,
) -> Result<Vec<HarnessFlowEvent>> {
    let db_path = root.join("runtime").join("ctox.sqlite3");
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open {}", db_path.display()))?;
    let has_table = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'ctox_harness_flow_events'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if !has_table {
        return Ok(Vec::new());
    }

    let limit = limit.min(500) as i64;
    let mut stmt = conn.prepare(
        "SELECT event_id, chain_key, event_kind, title, body_text,
                message_key, work_id, ticket_key, attempt_index, metadata_json, created_at
         FROM ctox_harness_flow_events
         WHERE (?1 IS NOT NULL AND message_key = ?1)
            OR (?2 IS NOT NULL AND work_id = ?2)
            OR (?3 IS NOT NULL AND ticket_key = ?3)
            OR (?1 IS NULL AND ?2 IS NULL AND ?3 IS NULL)
         ORDER BY created_at ASC
         LIMIT ?4",
    )?;
    let rows = stmt.query_map(params![message_key, work_id, ticket_key, limit], |row| {
        Ok(HarnessFlowEvent {
            event_id: row.get(0)?,
            chain_key: row.get(1)?,
            event_kind: row.get(2)?,
            title: row.get(3)?,
            body_text: row.get(4)?,
            message_key: row.get(5)?,
            work_id: row.get(6)?,
            ticket_key: row.get(7)?,
            attempt_index: row.get(8)?,
            metadata_json: row.get(9)?,
            created_at: row.get(10)?,
        })
    })?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

#[allow(dead_code)]
fn chain_key(message_key: Option<&str>, work_id: Option<&str>, ticket_key: Option<&str>) -> String {
    if let Some(message_key) = message_key.filter(|value| !value.trim().is_empty()) {
        format!("message:{message_key}")
    } else if let Some(work_id) = work_id.filter(|value| !value.trim().is_empty()) {
        format!("work:{work_id}")
    } else if let Some(ticket_key) = ticket_key.filter(|value| !value.trim().is_empty()) {
        format!("ticket:{ticket_key}")
    } else {
        "runtime".to_string()
    }
}

#[allow(dead_code)]
fn event_id(chain_key: &str, kind: &str, title: &str, body_text: &str, created_at: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(chain_key.as_bytes());
    hasher.update(b"\0");
    hasher.update(kind.as_bytes());
    hasher.update(b"\0");
    hasher.update(title.as_bytes());
    hasher.update(b"\0");
    hasher.update(body_text.as_bytes());
    hasher.update(b"\0");
    hasher.update(created_at.as_bytes());
    let digest = hasher.finalize();
    format!("hfe-{}", hex_prefix(&digest, 12))
}

#[allow(dead_code)]
fn hex_prefix(bytes: &[u8], len: usize) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .take(len)
        .map(|nibble| char::from_digit(nibble as u32, 16).unwrap_or('0'))
        .collect()
}

fn optional_string<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Option<String>> {
    Ok(conn.query_row(sql, params, |row| row.get(0)).ok())
}

fn optional_count(conn: &Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0)).unwrap_or(0))
}

fn optional_count_with_param(conn: &Connection, sql: &str, param: &str) -> Result<i64> {
    Ok(conn
        .query_row(sql, params![param], |row| row.get(0))
        .unwrap_or(0))
}

fn parent_message_key(metadata: &str) -> Option<String> {
    let value: Value = serde_json::from_str(metadata).ok()?;
    ["parent_message_key", "inbound_message_key"]
        .iter()
        .find_map(|key| value.get(*key)?.as_str().map(ToOwned::to_owned))
}

fn parse_string_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(idx, arg)| {
        if arg == name {
            args.get(idx + 1).map(String::as_str)
        } else {
            arg.strip_prefix(&format!("{name}="))
        }
    })
}

fn parse_usize_flag(args: &[String], name: &str, default: usize) -> usize {
    parse_string_flag(args, name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn first_non_empty<'a>(values: &[&'a str]) -> Option<&'a str> {
    values
        .iter()
        .copied()
        .find(|value| !value.trim().is_empty())
}

fn non_empty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

fn sentence_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn short_time(value: &str) -> String {
    value
        .split('T')
        .nth(1)
        .map(|time| time.trim_end_matches('Z').to_string())
        .unwrap_or_else(|| value.to_string())
}

fn clip(value: &str, max: usize) -> String {
    let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= max {
        cleaned
    } else {
        let take = max.saturating_sub(3);
        format!("{}...", cleaned.chars().take(take).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_keeps_support_branches_off_the_main_box() {
        let flow = HarnessFlow {
            schema_version: 1,
            source: FlowSource {
                message_key: Some("msg-1".to_string()),
                work_id: None,
                source_kind: "message".to_string(),
            },
            ledger_events: Vec::new(),
            blocks: vec![MainBlock {
                kind: FlowBlockKind::Task,
                title: "TASK".to_string(),
                lines: vec!["Answer the message.".to_string()],
                branches: vec![SupportBranch {
                    kind: FlowBranchKind::Review,
                    title: "REVIEW".to_string(),
                    lines: vec!["Result: do not send.".to_string()],
                    returns_to_spine: false,
                }],
            }],
        };
        let rendered = render_ascii(&flow, 110);
        assert!(rendered.contains("TASK"));
        assert!(rendered.contains("REVIEW"));
        assert!(rendered.contains("├──►"));
    }
}
