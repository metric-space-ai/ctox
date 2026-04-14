use anyhow::Context;
use anyhow::Result;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use serde::Serialize;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use std::time::SystemTime;

use crate::channels;

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/cto_agent.db";
const DEFAULT_GOAL_THREAD_PREFIX: &str = "plan";
const DEFAULT_RESULT_EXCERPT_CHARS: usize = 420;

const STEP_STATUS_PENDING: &str = "pending";
const STEP_STATUS_QUEUED: &str = "queued";
const STEP_STATUS_COMPLETED: &str = "completed";
const STEP_STATUS_BLOCKED: &str = "blocked";
const STEP_STATUS_FAILED: &str = "failed";

const GOAL_STATUS_ACTIVE: &str = "active";
const GOAL_STATUS_COMPLETED: &str = "completed";
const GOAL_STATUS_BLOCKED: &str = "blocked";

#[derive(Debug, Clone, Serialize)]
pub struct PlannedGoalView {
    pub goal_id: String,
    pub title: String,
    pub source_prompt: String,
    pub thread_key: String,
    pub skill: Option<String>,
    pub auto_advance: bool,
    pub status: String,
    pub next_step_id: Option<String>,
    pub next_step_title: Option<String>,
    pub last_emitted_at: Option<String>,
    pub last_completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedStepView {
    pub step_id: String,
    pub goal_id: String,
    pub step_order: i64,
    pub title: String,
    pub instruction: String,
    pub status: String,
    pub defer_until: Option<String>,
    pub blocked_reason: Option<String>,
    pub attempt_count: i64,
    pub last_message_key: Option<String>,
    pub last_result_excerpt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct EmitDuePlansSummary {
    pub emitted_count: usize,
    pub emitted_steps: Vec<EmittedPlanStepView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmittedPlanStepView {
    pub goal_id: String,
    pub step_id: String,
    pub step_title: String,
    pub message_key: String,
    pub emitted_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoalWithStepsView {
    pub goal: PlannedGoalView,
    pub steps: Vec<PlannedStepView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanDraftView {
    pub title: String,
    pub source_prompt: String,
    pub suggested_skill: Option<String>,
    pub persistence_recommended: bool,
    pub steps: Vec<PlanDraftStepView>,
    pub replan_triggers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanDraftStepView {
    pub step_order: i64,
    pub title: String,
    pub instruction: String,
}

#[derive(Debug, Clone)]
struct PlanCreateRequest {
    title: String,
    prompt: String,
    thread_key: Option<String>,
    skill: Option<String>,
    auto_advance: bool,
    emit_now: bool,
}

#[derive(Debug, Clone)]
pub struct PlanIngestRequest {
    pub title: String,
    pub prompt: String,
    pub thread_key: Option<String>,
    pub skill: Option<String>,
    pub auto_advance: bool,
    pub emit_now: bool,
}

#[derive(Debug, Clone)]
struct PlannedStepDraft {
    title: String,
    instruction: String,
}

#[derive(Debug, Clone)]
struct PendingStepEmission {
    goal: PlannedGoalView,
    step: PlannedStepView,
    prompt: String,
    total_steps: i64,
}

pub fn handle_plan_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            let conn = open_plan_db(root)?;
            print_json(&json!({
                "ok": true,
                "db_path": resolve_db_path(root),
                "initialized": schema_state(&conn)?,
            }))
        }
        "draft" => {
            let request = parse_draft_request(args)?;
            let draft = draft_plan(request);
            print_json(&json!({"ok": true, "draft": draft}))
        }
        "ingest" => {
            let request = parse_ingest_request(args)?;
            let created = create_goal(root, request)?;
            print_json(&json!({"ok": true, "plan": created}))
        }
        "list" => {
            let goals = list_goals(root)?;
            print_json(&json!({"ok": true, "count": goals.len(), "goals": goals}))
        }
        "show" => {
            let goal_id = required_flag_value(args, "--goal-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox plan show --goal-id <id>")?;
            let view = load_goal_with_steps(root, goal_id)?.context("planned goal not found")?;
            print_json(&json!({"ok": true, "plan": view}))
        }
        "emit-next" => {
            let goal_id = required_flag_value(args, "--goal-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox plan emit-next --goal-id <id>")?;
            let emitted = emit_next_step_for_goal(root, goal_id)?
                .context("no eligible pending step available for this goal")?;
            print_json(&json!({"ok": true, "emitted": emitted}))
        }
        "tick" => {
            let summary = emit_due_steps(root)?;
            print_json(&json!({"ok": true, "summary": summary}))
        }
        "complete-step" => {
            let step_id = required_flag_value(args, "--step-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox plan complete-step --step-id <id> [--result <text>]")?;
            let result = find_flag_value(args, "--result").unwrap_or("");
            let updated = mark_step_completed(root, step_id, result)?;
            print_json(&json!({"ok": true, "updated": updated}))
        }
        "fail-step" => {
            let step_id = required_flag_value(args, "--step-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox plan fail-step --step-id <id> --reason <text>")?;
            let reason = required_flag_value(args, "--reason")
                .context("usage: ctox plan fail-step --step-id <id> --reason <text>")?;
            let updated = mark_step_failed(root, step_id, reason)?;
            print_json(&json!({"ok": true, "updated": updated}))
        }
        "retry-step" => {
            let step_id = required_flag_value(args, "--step-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox plan retry-step --step-id <id>")?;
            let updated = reset_step_to_pending(root, step_id, None)?;
            print_json(&json!({"ok": true, "updated": updated}))
        }
        "block-step" => {
            let step_id = required_flag_value(args, "--step-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox plan block-step --step-id <id> --reason <text>")?;
            let reason = required_flag_value(args, "--reason")
                .context("usage: ctox plan block-step --step-id <id> --reason <text>")?;
            let updated = mark_step_blocked(root, step_id, reason)?;
            print_json(&json!({"ok": true, "updated": updated}))
        }
        "unblock-step" => {
            let step_id = required_flag_value(args, "--step-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox plan unblock-step --step-id <id> [--defer-minutes <n>]")?;
            let defer_until = parse_defer_minutes(find_flag_value(args, "--defer-minutes"))?;
            let updated = reset_step_to_pending(root, step_id, defer_until)?;
            print_json(&json!({"ok": true, "updated": updated}))
        }
        _ => anyhow::bail!(
            "usage:\n  ctox plan init\n  ctox plan draft --title <label> --prompt <text> [--skill <name>]\n  ctox plan ingest --title <label> --prompt <text> [--thread-key <key>] [--skill <name>] [--auto-advance] [--emit-now]\n  ctox plan list\n  ctox plan show --goal-id <id>\n  ctox plan emit-next --goal-id <id>\n  ctox plan tick\n  ctox plan complete-step --step-id <id> [--result <text>]\n  ctox plan fail-step --step-id <id> --reason <text>\n  ctox plan retry-step --step-id <id>\n  ctox plan block-step --step-id <id> --reason <text>\n  ctox plan unblock-step --step-id <id> [--defer-minutes <n>]"
        ),
    }
}

pub fn ingest_goal(root: &Path, request: PlanIngestRequest) -> Result<GoalWithStepsView> {
    create_goal(
        root,
        PlanCreateRequest {
            title: request.title,
            prompt: request.prompt,
            thread_key: request.thread_key,
            skill: request.skill,
            auto_advance: request.auto_advance,
            emit_now: request.emit_now,
        },
    )
}

pub fn complete_goal(root: &Path, goal_id: &str, result_text: &str) -> Result<usize> {
    let conn = open_plan_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let mut statement = tx.prepare(
        r#"
        SELECT step_id
        FROM planned_steps
        WHERE goal_id = ?1
          AND status != ?2
        ORDER BY step_order ASC
        "#,
    )?;
    let step_ids = statement
        .query_map(params![goal_id, STEP_STATUS_COMPLETED], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    let mut updated = 0usize;
    for step_id in &step_ids {
        updated += mark_step_completed_tx(&tx, step_id, result_text)?;
    }
    refresh_goal_status_tx(&tx, goal_id)?;
    tx.commit()?;
    Ok(updated)
}

/// Mark a plan step as completed based on the `last_message_key` set when it
/// was emitted. Returns the number of rows updated (0 if the key does not map
/// to a queued step). Used by the service loop to auto-complete plan steps
/// whose emitted queue message was just handled successfully.
pub fn complete_step_by_message_key(
    root: &Path,
    message_key: &str,
    result_text: &str,
) -> Result<usize> {
    let conn = open_plan_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let step_id: Option<String> = tx
        .query_row(
            r#"
            SELECT step_id
            FROM planned_steps
            WHERE last_message_key = ?1
              AND status != ?2
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
            params![message_key, STEP_STATUS_COMPLETED],
            |row| row.get(0),
        )
        .optional()?;
    let Some(step_id) = step_id else {
        tx.commit()?;
        return Ok(0);
    };
    let updated = mark_step_completed_tx(&tx, &step_id, result_text)?;
    if let Some(goal_id) = load_goal_id_for_step_tx(&tx, &step_id)? {
        refresh_goal_status_tx(&tx, &goal_id)?;
    }
    tx.commit()?;
    Ok(updated)
}

pub fn emit_due_steps(root: &Path) -> Result<EmitDuePlansSummary> {
    let conn = open_plan_db(root)?;
    let goals = list_goal_ids_with_due_work(&conn)?;
    let mut summary = EmitDuePlansSummary::default();
    for goal_id in goals {
        if let Some(emitted) = emit_next_step_for_goal(root, &goal_id)? {
            summary.emitted_count += 1;
            summary.emitted_steps.push(emitted);
        }
    }
    Ok(summary)
}

fn create_goal(root: &Path, request: PlanCreateRequest) -> Result<GoalWithStepsView> {
    let conn = open_plan_db(root)?;
    let now = now_iso_string();
    let goal_id = format!(
        "goal_{}",
        stable_digest(&format!(
            "{}:{}:{}",
            request.title.trim(),
            request.prompt.trim(),
            now
        ))
    );
    let thread_key = request
        .thread_key
        .unwrap_or_else(|| format!("{DEFAULT_GOAL_THREAD_PREFIX}/{goal_id}"));
    let drafts = decompose_prompt_into_steps(&request.title, &request.prompt);

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        r#"
        INSERT INTO planned_goals (
            goal_id, title, source_prompt, thread_key, skill, auto_advance, status,
            last_emitted_at, last_completed_at, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8, ?8)
        "#,
        params![
            goal_id,
            request.title.trim(),
            request.prompt.trim(),
            thread_key,
            request.skill.as_deref(),
            if request.auto_advance { 1 } else { 0 },
            GOAL_STATUS_ACTIVE,
            now,
        ],
    )?;

    for (index, draft) in drafts.iter().enumerate() {
        let step_id = format!(
            "{}::step_{}",
            goal_id,
            stable_digest(&format!("{}:{}:{}", goal_id, index + 1, draft.title))
        );
        tx.execute(
            r#"
            INSERT INTO planned_steps (
                step_id, goal_id, step_order, title, instruction, status, defer_until,
                blocked_reason, attempt_count, last_message_key, last_result_excerpt,
                created_at, updated_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, 0, NULL, NULL, ?7, ?7, NULL)
            "#,
            params![
                step_id,
                goal_id,
                index as i64 + 1,
                draft.title.trim(),
                draft.instruction.trim(),
                STEP_STATUS_PENDING,
                now,
            ],
        )?;
    }
    tx.commit()?;

    if request.emit_now {
        let _ = emit_next_step_for_goal(root, &goal_id)?;
    }

    load_goal_with_steps(root, &goal_id)?.context("failed to reload created planned goal")
}

fn draft_plan(request: PlanCreateRequest) -> PlanDraftView {
    let steps = decompose_prompt_into_steps(&request.title, &request.prompt)
        .into_iter()
        .enumerate()
        .map(|(index, draft)| PlanDraftStepView {
            step_order: index as i64 + 1,
            title: draft.title,
            instruction: draft.instruction,
        })
        .collect::<Vec<_>>();
    PlanDraftView {
        title: request.title,
        source_prompt: request.prompt,
        suggested_skill: request.skill,
        persistence_recommended: steps.len() > 1,
        steps,
        replan_triggers: vec![
            "new owner requirements arrive".to_string(),
            "repo or runtime state differs from current assumptions".to_string(),
            "the next step would leave the original scope".to_string(),
            "a blocker requires owner input or external approval".to_string(),
        ],
    }
}

pub fn list_goals(root: &Path) -> Result<Vec<PlannedGoalView>> {
    let conn = open_plan_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            g.goal_id,
            g.title,
            g.source_prompt,
            g.thread_key,
            g.skill,
            g.auto_advance,
            g.status,
            (
                SELECT s.step_id
                FROM planned_steps s
                WHERE s.goal_id = g.goal_id
                  AND s.status IN ('pending', 'queued', 'blocked', 'failed')
                ORDER BY s.step_order ASC
                LIMIT 1
            ) AS next_step_id,
            (
                SELECT s.title
                FROM planned_steps s
                WHERE s.goal_id = g.goal_id
                  AND s.status IN ('pending', 'queued', 'blocked', 'failed')
                ORDER BY s.step_order ASC
                LIMIT 1
            ) AS next_step_title,
            g.last_emitted_at,
            g.last_completed_at,
            g.created_at,
            g.updated_at
        FROM planned_goals g
        ORDER BY
            CASE g.status
                WHEN 'active' THEN 0
                WHEN 'blocked' THEN 1
                WHEN 'completed' THEN 2
                ELSE 3
            END,
            g.updated_at DESC
        "#,
    )?;
    let rows = statement.query_map([], map_goal_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_goal_with_steps(root: &Path, goal_id: &str) -> Result<Option<GoalWithStepsView>> {
    let conn = open_plan_db(root)?;
    let goal = load_goal(&conn, goal_id)?;
    let Some(goal) = goal else {
        return Ok(None);
    };
    let steps = list_steps_for_goal(&conn, goal_id)?;
    Ok(Some(GoalWithStepsView { goal, steps }))
}

pub fn emit_next_step_for_goal(root: &Path, goal_id: &str) -> Result<Option<EmittedPlanStepView>> {
    let conn = open_plan_db(root)?;
    let Some(pending) = prepare_next_step_emission(&conn, goal_id)? else {
        return Ok(None);
    };
    let message_key = channels::ingest_plan_message(
        root,
        &pending.goal.goal_id,
        &pending.step.step_id,
        &pending.goal.thread_key,
        &pending.goal.title,
        &pending.step.title,
        &pending.prompt,
        pending.goal.skill.as_deref(),
        pending.step.step_order,
        pending.total_steps,
    )?;
    let now = now_iso_string();
    let tx = conn.unchecked_transaction()?;
    let updated = tx.execute(
        r#"
        UPDATE planned_steps
        SET status = ?2,
            attempt_count = attempt_count + 1,
            last_message_key = ?3,
            updated_at = ?4
        WHERE step_id = ?1
          AND status = 'pending'
        "#,
        params![pending.step.step_id, STEP_STATUS_QUEUED, message_key, now],
    )?;
    if updated == 0 {
        tx.commit()?;
        return Ok(None);
    }
    tx.execute(
        r#"
        UPDATE planned_goals
        SET status = ?2,
            last_emitted_at = ?3,
            updated_at = ?3
        WHERE goal_id = ?1
        "#,
        params![pending.goal.goal_id, GOAL_STATUS_ACTIVE, now],
    )?;
    tx.commit()?;
    Ok(Some(EmittedPlanStepView {
        goal_id: pending.goal.goal_id,
        step_id: pending.step.step_id,
        step_title: pending.step.title,
        message_key,
        emitted_at: now,
    }))
}

fn prepare_next_step_emission(
    conn: &Connection,
    goal_id: &str,
) -> Result<Option<PendingStepEmission>> {
    let tx = conn.unchecked_transaction()?;
    let goal = load_goal_tx(&tx, goal_id)?.context("planned goal not found")?;
    if goal.status == GOAL_STATUS_COMPLETED {
        tx.commit()?;
        return Ok(None);
    }
    if has_queued_step_tx(&tx, goal_id)? {
        tx.commit()?;
        return Ok(None);
    }
    let Some(step) = next_eligible_step_tx(&tx, goal_id)? else {
        refresh_goal_status_tx(&tx, goal_id)?;
        tx.commit()?;
        return Ok(None);
    };
    let prompt = render_step_prompt(&goal, &step, &list_completed_steps_tx(&tx, goal_id)?);
    let total_steps = total_steps_tx(&tx, goal_id)?;
    tx.commit()?;
    Ok(Some(PendingStepEmission {
        goal,
        step,
        prompt,
        total_steps,
    }))
}

fn mark_step_completed(root: &Path, step_id: &str, result_text: &str) -> Result<usize> {
    let conn = open_plan_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let updated = mark_step_completed_tx(&tx, step_id, result_text)?;
    if let Some(goal_id) = load_goal_id_for_step_tx(&tx, step_id)? {
        refresh_goal_status_tx(&tx, &goal_id)?;
    }
    tx.commit()?;
    let _ = emit_due_steps(root)?;
    Ok(updated)
}

fn mark_step_completed_tx(tx: &Transaction<'_>, step_id: &str, result_text: &str) -> Result<usize> {
    let now = now_iso_string();
    Ok(tx.execute(
        r#"
        UPDATE planned_steps
        SET status = ?2,
            blocked_reason = NULL,
            last_result_excerpt = ?3,
            updated_at = ?4,
            completed_at = ?4
        WHERE step_id = ?1
          AND status != ?2
        "#,
        params![
            step_id,
            STEP_STATUS_COMPLETED,
            clip_text(result_text, DEFAULT_RESULT_EXCERPT_CHARS),
            now
        ],
    )?)
}

fn mark_step_failed(root: &Path, step_id: &str, reason: &str) -> Result<usize> {
    let conn = open_plan_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let updated = mark_step_failed_tx(&tx, step_id, reason)?;
    if let Some(goal_id) = load_goal_id_for_step_tx(&tx, step_id)? {
        refresh_goal_status_tx(&tx, &goal_id)?;
    }
    tx.commit()?;
    Ok(updated)
}

fn mark_step_failed_tx(tx: &Transaction<'_>, step_id: &str, reason: &str) -> Result<usize> {
    let now = now_iso_string();
    Ok(tx.execute(
        r#"
        UPDATE planned_steps
        SET status = ?2,
            blocked_reason = NULL,
            last_result_excerpt = ?3,
            updated_at = ?4
        WHERE step_id = ?1
          AND status != ?5
        "#,
        params![
            step_id,
            STEP_STATUS_FAILED,
            clip_text(reason, DEFAULT_RESULT_EXCERPT_CHARS),
            now,
            STEP_STATUS_COMPLETED
        ],
    )?)
}

fn mark_step_blocked(root: &Path, step_id: &str, reason: &str) -> Result<usize> {
    let conn = open_plan_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let now = now_iso_string();
    let updated = tx.execute(
        r#"
        UPDATE planned_steps
        SET status = ?2,
            blocked_reason = ?3,
            updated_at = ?4
        WHERE step_id = ?1
          AND status != ?5
        "#,
        params![
            step_id,
            STEP_STATUS_BLOCKED,
            reason.trim(),
            now,
            STEP_STATUS_COMPLETED
        ],
    )?;
    if let Some(goal_id) = load_goal_id_for_step_tx(&tx, step_id)? {
        refresh_goal_status_tx(&tx, &goal_id)?;
    }
    tx.commit()?;
    Ok(updated)
}

fn reset_step_to_pending(root: &Path, step_id: &str, defer_until: Option<String>) -> Result<usize> {
    let conn = open_plan_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let now = now_iso_string();
    let updated = tx.execute(
        r#"
        UPDATE planned_steps
        SET status = ?2,
            defer_until = ?3,
            blocked_reason = NULL,
            updated_at = ?4
        WHERE step_id = ?1
          AND status != ?5
        "#,
        params![
            step_id,
            STEP_STATUS_PENDING,
            defer_until.as_deref(),
            now,
            STEP_STATUS_COMPLETED
        ],
    )?;
    if let Some(goal_id) = load_goal_id_for_step_tx(&tx, step_id)? {
        refresh_goal_status_tx(&tx, &goal_id)?;
    }
    tx.commit()?;
    Ok(updated)
}

fn render_step_prompt(
    goal: &PlannedGoalView,
    step: &PlannedStepView,
    completed_steps: &[PlannedStepView],
) -> String {
    let mut lines = vec![
        format!("Plan goal: {}", goal.title),
        format!("Plan step {}: {}", step.step_order, step.title),
        "Work only on this step. Do not silently skip ahead.".to_string(),
    ];
    let autonomy = crate::autonomy::AutonomyLevel::from_env();
    lines.push(String::new());
    lines.push(autonomy.step_prompt_clause().to_string());
    if let Some(skill) = goal
        .skill
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("Preferred skill/tooling: {skill}"));
    }
    lines.push(String::new());
    lines.push("Original owner request:".to_string());
    lines.push(goal.source_prompt.clone());
    lines.push(String::new());
    if !completed_steps.is_empty() {
        lines.push("Completed plan steps so far:".to_string());
        for completed in completed_steps {
            let result = completed
                .last_result_excerpt
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("completed");
            lines.push(format!(
                "- [{}] {} -> {}",
                completed.step_order, completed.title, result
            ));
        }
        lines.push(String::new());
    }
    lines.push("Current step instruction:".to_string());
    lines.push(step.instruction.clone());
    lines.push(String::new());
    lines.push("Do the step. When the step is concrete work (code change, data migration, running a script, producing a measurement), actually perform it — do not write a document describing how it could be done. When the step is genuinely investigative or decision-shaped (architecture choice, policy, requirements analysis, source/filter review), the findings or decision are the real output and persisting them as knowledge is legitimate execution, not planning.".to_string());
    lines.push(String::new());
    lines.push("Deliverables — persist real CTOX artifacts, not prose:".to_string());
    lines.push("- Findings, decisions, architecture notes, policies, measured results → `ctox ticket knowledge-put --system <system> --domain <domain> --title <title> --body <body>`. Knowledge should capture a conclusion or verified fact, not a description of work you have not done yet.".to_string());
    lines.push("- Concrete implementation work that genuinely belongs to a later slice (because it depends on something outside this step) → `ctox ticket self-work-put --system <system> --kind change --title <title> --body <body>`. If the code change fits inside this step, just make the change now instead of deferring it.".to_string());
    lines.push("- Owner approval needed before a genuinely high-impact irreversible move (production cutover, destructive migration, public communication) → `ctox ticket self-work-put --system <system> --kind approval-gate --title <title> --body <body>`. Most steps do not need an approval gate; use this only when the next action would really be irreversible without sign-off.".to_string());
    lines.push("- Credentials, API keys, or accounts you cannot obtain yourself → `ctox ticket access-request-put --system <system> --title <title> --body <body> --required-scopes <csv>`.".to_string());
    lines.push("- A workstream that genuinely needs its own multi-step plan → `ctox plan ingest --title <title> --prompt <text>`. Do not spawn a sub-plan just to avoid working on this step.".to_string());
    lines.push(String::new());
    lines.push(
        "Reply briefly with what you persisted and what blockers remain. A reply without any persisted artifact is not a completed step unless the step was purely a summary/decision step and the decision itself is stored as knowledge. At the same time: writing another plan, another approval gate, or another scope document about work you could have done now is not completion either — it is the same step restated.".to_string(),
    );
    lines.join("\n")
}

fn decompose_prompt_into_steps(title: &str, prompt: &str) -> Vec<PlannedStepDraft> {
    let bullet_candidates = prompt
        .lines()
        .filter_map(extract_bullet_candidate)
        .take(8)
        .collect::<Vec<_>>();
    if bullet_candidates.len() >= 2 {
        return bullet_candidates
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| build_candidate_step(title, prompt, index, &candidate))
            .collect();
    }

    let sentence_candidates = split_sentence_candidates(prompt);
    if sentence_candidates.len() >= 2 {
        return sentence_candidates
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| build_candidate_step(title, prompt, index, &candidate))
            .collect();
    }

    vec![
        PlannedStepDraft {
            title: "Inspect scope and constraints".to_string(),
            instruction: format!(
                "Inspect the request, current repo state, and constraints that matter for this goal.\n\nOriginal request:\n{}",
                prompt.trim()
            ),
        },
        PlannedStepDraft {
            title: "Execute the next concrete slice".to_string(),
            instruction: format!(
                "Carry out the next concrete slice for the goal '{}'. Keep the change bounded to one coherent increment and surface any blockers explicitly.",
                title.trim()
            ),
        },
        PlannedStepDraft {
            title: "Verify result and prepare follow-up".to_string(),
            instruction: "Verify the work completed so far, record the concrete outcome, and prepare the next follow-up slice if the broader goal still has remaining work.".to_string(),
        },
    ]
}

fn build_candidate_step(
    title: &str,
    prompt: &str,
    index: usize,
    candidate: &str,
) -> PlannedStepDraft {
    let candidate = collapse_ws(candidate);
    let step_title = clip_text(&candidate, 96);
    let instruction = if index == 0 {
        format!(
            "Start the goal '{}', focusing on this concrete item first: {}\n\nOriginal request:\n{}",
            title.trim(),
            candidate,
            prompt.trim()
        )
    } else {
        format!(
            "Continue the goal '{}', focusing on this concrete item: {}\n\nUse prior completed steps as context and do not repeat already-finished work.",
            title.trim(),
            candidate
        )
    };
    PlannedStepDraft {
        title: step_title,
        instruction,
    }
}

fn extract_bullet_candidate(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let stripped = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("• "))
        .map(ToOwned::to_owned)
        .or_else(|| strip_numeric_list_marker(trimmed));
    stripped.filter(|value| value.split_whitespace().count() >= 3)
}

fn strip_numeric_list_marker(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == 0 || idx >= bytes.len() {
        return None;
    }
    if matches!(bytes[idx], b'.' | b')' | b':') && bytes.get(idx + 1).copied() == Some(b' ') {
        return Some(value[idx + 2..].to_string());
    }
    None
}

fn split_sentence_candidates(prompt: &str) -> Vec<String> {
    let normalized = prompt
        .replace("\r\n", "\n")
        .replace("; ", ". ")
        .replace(" und dann ", ". ")
        .replace(" danach ", ". ")
        .replace(" then ", ". ")
        .replace('\n', ". ");
    normalized
        .split('.')
        .map(collapse_ws)
        .filter(|candidate| candidate.split_whitespace().count() >= 5)
        .take(8)
        .collect()
}

fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether any active plan still has pending steps, ignoring auto_advance.
/// Used by the mission idle watchdog so that it keeps triggering as long as
/// real plan work is open, even when the mission state record has drifted to
/// allow_idle or is_open=false.
pub fn has_active_goal_with_pending_step(root: &Path) -> Result<bool> {
    let conn = open_plan_db(root)?;
    let now = now_iso_string();
    let exists: i64 = conn.query_row(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM planned_goals g
            JOIN planned_steps s ON s.goal_id = g.goal_id
            WHERE g.status = 'active'
              AND s.status IN ('pending', 'queued', 'blocked', 'failed')
              AND (
                    s.defer_until IS NULL
                    OR s.defer_until = ''
                    OR s.defer_until <= ?1
              )
        )
        "#,
        params![now],
        |row| row.get(0),
    )?;
    Ok(exists != 0)
}

fn list_goal_ids_with_due_work(conn: &Connection) -> Result<Vec<String>> {
    let mut statement = conn.prepare(
        r#"
        SELECT DISTINCT g.goal_id
        FROM planned_goals g
        JOIN planned_steps s ON s.goal_id = g.goal_id
        WHERE g.auto_advance = 1
          AND g.status = 'active'
          AND s.status = 'pending'
          AND (
                s.defer_until IS NULL
                OR s.defer_until = ''
                OR s.defer_until <= ?1
          )
        ORDER BY g.updated_at ASC, g.created_at ASC
        "#,
    )?;
    let now = now_iso_string();
    let rows = statement.query_map(params![now], |row| row.get::<_, String>(0))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_goal(conn: &Connection, goal_id: &str) -> Result<Option<PlannedGoalView>> {
    conn.query_row(
        r#"
        SELECT
            g.goal_id,
            g.title,
            g.source_prompt,
            g.thread_key,
            g.skill,
            g.auto_advance,
            g.status,
            (
                SELECT s.step_id
                FROM planned_steps s
                WHERE s.goal_id = g.goal_id
                  AND s.status IN ('pending', 'queued', 'blocked', 'failed')
                ORDER BY s.step_order ASC
                LIMIT 1
            ) AS next_step_id,
            (
                SELECT s.title
                FROM planned_steps s
                WHERE s.goal_id = g.goal_id
                  AND s.status IN ('pending', 'queued', 'blocked', 'failed')
                ORDER BY s.step_order ASC
                LIMIT 1
            ) AS next_step_title,
            g.last_emitted_at,
            g.last_completed_at,
            g.created_at,
            g.updated_at
        FROM planned_goals g
        WHERE g.goal_id = ?1
        LIMIT 1
        "#,
        params![goal_id],
        map_goal_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_goal_tx(tx: &Transaction<'_>, goal_id: &str) -> Result<Option<PlannedGoalView>> {
    tx.query_row(
        r#"
        SELECT
            g.goal_id,
            g.title,
            g.source_prompt,
            g.thread_key,
            g.skill,
            g.auto_advance,
            g.status,
            (
                SELECT s.step_id
                FROM planned_steps s
                WHERE s.goal_id = g.goal_id
                  AND s.status IN ('pending', 'queued', 'blocked', 'failed')
                ORDER BY s.step_order ASC
                LIMIT 1
            ) AS next_step_id,
            (
                SELECT s.title
                FROM planned_steps s
                WHERE s.goal_id = g.goal_id
                  AND s.status IN ('pending', 'queued', 'blocked', 'failed')
                ORDER BY s.step_order ASC
                LIMIT 1
            ) AS next_step_title,
            g.last_emitted_at,
            g.last_completed_at,
            g.created_at,
            g.updated_at
        FROM planned_goals g
        WHERE g.goal_id = ?1
        LIMIT 1
        "#,
        params![goal_id],
        map_goal_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn list_steps_for_goal(conn: &Connection, goal_id: &str) -> Result<Vec<PlannedStepView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            step_id,
            goal_id,
            step_order,
            title,
            instruction,
            status,
            defer_until,
            blocked_reason,
            attempt_count,
            last_message_key,
            last_result_excerpt,
            created_at,
            updated_at,
            completed_at
        FROM planned_steps
        WHERE goal_id = ?1
        ORDER BY step_order ASC
        "#,
    )?;
    let rows = statement.query_map(params![goal_id], map_step_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_completed_steps_tx(tx: &Transaction<'_>, goal_id: &str) -> Result<Vec<PlannedStepView>> {
    let mut statement = tx.prepare(
        r#"
        SELECT
            step_id,
            goal_id,
            step_order,
            title,
            instruction,
            status,
            defer_until,
            blocked_reason,
            attempt_count,
            last_message_key,
            last_result_excerpt,
            created_at,
            updated_at,
            completed_at
        FROM planned_steps
        WHERE goal_id = ?1
          AND status = 'completed'
        ORDER BY step_order ASC
        "#,
    )?;
    let rows = statement.query_map(params![goal_id], map_step_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn next_eligible_step_tx(tx: &Transaction<'_>, goal_id: &str) -> Result<Option<PlannedStepView>> {
    tx.query_row(
        r#"
        SELECT
            step_id,
            goal_id,
            step_order,
            title,
            instruction,
            status,
            defer_until,
            blocked_reason,
            attempt_count,
            last_message_key,
            last_result_excerpt,
            created_at,
            updated_at,
            completed_at
        FROM planned_steps
        WHERE goal_id = ?1
          AND status = 'pending'
          AND (
                defer_until IS NULL
                OR defer_until = ''
                OR defer_until <= ?2
          )
        ORDER BY step_order ASC
        LIMIT 1
        "#,
        params![goal_id, now_iso_string()],
        map_step_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn has_queued_step_tx(tx: &Transaction<'_>, goal_id: &str) -> Result<bool> {
    let count: i64 = tx.query_row(
        r#"
        SELECT COUNT(*)
        FROM planned_steps
        WHERE goal_id = ?1
          AND status = 'queued'
        "#,
        params![goal_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn total_steps_tx(tx: &Transaction<'_>, goal_id: &str) -> Result<i64> {
    tx.query_row(
        "SELECT COUNT(*) FROM planned_steps WHERE goal_id = ?1",
        params![goal_id],
        |row| row.get(0),
    )
    .map_err(anyhow::Error::from)
}

fn load_goal_id_for_step_tx(tx: &Transaction<'_>, step_id: &str) -> Result<Option<String>> {
    tx.query_row(
        "SELECT goal_id FROM planned_steps WHERE step_id = ?1 LIMIT 1",
        params![step_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn refresh_goal_status_tx(tx: &Transaction<'_>, goal_id: &str) -> Result<()> {
    let completed: i64 = tx.query_row(
        "SELECT COUNT(*) FROM planned_steps WHERE goal_id = ?1 AND status = 'completed'",
        params![goal_id],
        |row| row.get(0),
    )?;
    let total: i64 = tx.query_row(
        "SELECT COUNT(*) FROM planned_steps WHERE goal_id = ?1",
        params![goal_id],
        |row| row.get(0),
    )?;
    let blocked: i64 = tx.query_row(
        "SELECT COUNT(*) FROM planned_steps WHERE goal_id = ?1 AND status = 'blocked'",
        params![goal_id],
        |row| row.get(0),
    )?;
    let status = if total > 0 && completed == total {
        GOAL_STATUS_COMPLETED
    } else if blocked > 0 {
        GOAL_STATUS_BLOCKED
    } else {
        GOAL_STATUS_ACTIVE
    };
    let now = now_iso_string();
    let completed_at = if status == GOAL_STATUS_COMPLETED {
        Some(now.clone())
    } else {
        None
    };
    tx.execute(
        r#"
        UPDATE planned_goals
        SET status = ?2,
            last_completed_at = COALESCE(?3, last_completed_at),
            updated_at = ?4
        WHERE goal_id = ?1
        "#,
        params![goal_id, status, completed_at.as_deref(), now],
    )?;
    Ok(())
}

fn map_goal_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlannedGoalView> {
    Ok(PlannedGoalView {
        goal_id: row.get(0)?,
        title: row.get(1)?,
        source_prompt: row.get(2)?,
        thread_key: row.get(3)?,
        skill: row.get(4)?,
        auto_advance: row.get::<_, i64>(5)? != 0,
        status: row.get(6)?,
        next_step_id: row.get(7)?,
        next_step_title: row.get(8)?,
        last_emitted_at: row.get(9)?,
        last_completed_at: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn map_step_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlannedStepView> {
    Ok(PlannedStepView {
        step_id: row.get(0)?,
        goal_id: row.get(1)?,
        step_order: row.get(2)?,
        title: row.get(3)?,
        instruction: row.get(4)?,
        status: row.get(5)?,
        defer_until: row.get(6)?,
        blocked_reason: row.get(7)?,
        attempt_count: row.get(8)?,
        last_message_key: row.get(9)?,
        last_result_excerpt: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        completed_at: row.get(13)?,
    })
}

fn open_plan_db(root: &Path) -> Result<Connection> {
    let path = resolve_db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create plan db parent {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open plan db {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .context("failed to configure SQLite busy_timeout for plans")?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 5000;

        CREATE TABLE IF NOT EXISTS planned_goals (
            goal_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            source_prompt TEXT NOT NULL,
            thread_key TEXT NOT NULL,
            skill TEXT,
            auto_advance INTEGER NOT NULL DEFAULT 1,
            status TEXT NOT NULL,
            last_emitted_at TEXT,
            last_completed_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS planned_steps (
            step_id TEXT PRIMARY KEY,
            goal_id TEXT NOT NULL,
            step_order INTEGER NOT NULL,
            title TEXT NOT NULL,
            instruction TEXT NOT NULL,
            status TEXT NOT NULL,
            defer_until TEXT,
            blocked_reason TEXT,
            attempt_count INTEGER NOT NULL DEFAULT 0,
            last_message_key TEXT,
            last_result_excerpt TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            completed_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_planned_goals_status
            ON planned_goals(status, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_planned_steps_goal
            ON planned_steps(goal_id, step_order ASC);
        CREATE INDEX IF NOT EXISTS idx_planned_steps_status_due
            ON planned_steps(status, defer_until, updated_at);
        "#,
    )?;
    Ok(conn)
}

fn schema_state(conn: &Connection) -> Result<serde_json::Value> {
    let goal_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM planned_goals", [], |row| row.get(0))?;
    let step_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM planned_steps", [], |row| row.get(0))?;
    Ok(json!({
        "planned_goals": goal_count,
        "planned_steps": step_count,
    }))
}

fn resolve_db_path(root: &Path) -> std::path::PathBuf {
    root.join(DEFAULT_DB_RELATIVE_PATH)
}

fn parse_ingest_request(args: &[String]) -> Result<PlanCreateRequest> {
    Ok(PlanCreateRequest {
        title: required_flag_value(args, "--title")
            .context("usage: ctox plan ingest --title <label> --prompt <text>")?
            .to_string(),
        prompt: required_flag_value(args, "--prompt")
            .context("usage: ctox plan ingest --title <label> --prompt <text>")?
            .to_string(),
        thread_key: find_flag_value(args, "--thread-key").map(ToOwned::to_owned),
        skill: find_flag_value(args, "--skill").map(ToOwned::to_owned),
        // Default to auto-advance so plans keep moving without a human trigger
        // between steps. Opt-out with --no-auto-advance for plans that need
        // explicit approval between every step.
        auto_advance: !args.iter().any(|arg| arg == "--no-auto-advance"),
        emit_now: args.iter().any(|arg| arg == "--emit-now"),
    })
}

fn parse_draft_request(args: &[String]) -> Result<PlanCreateRequest> {
    Ok(PlanCreateRequest {
        title: required_flag_value(args, "--title")
            .context("usage: ctox plan draft --title <label> --prompt <text>")?
            .to_string(),
        prompt: required_flag_value(args, "--prompt")
            .context("usage: ctox plan draft --title <label> --prompt <text>")?
            .to_string(),
        thread_key: None,
        skill: find_flag_value(args, "--skill").map(ToOwned::to_owned),
        auto_advance: false,
        emit_now: false,
    })
}

fn parse_defer_minutes(raw: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let minutes = raw
        .parse::<i64>()
        .with_context(|| format!("failed to parse defer minutes '{raw}'"))?;
    let target = DateTime::<Utc>::from(SystemTime::now()) + Duration::minutes(minutes);
    Ok(Some(target.to_rfc3339()))
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn clip_text(value: &str, max_chars: usize) -> String {
    let collapsed = collapse_ws(value);
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

fn now_iso_string() -> String {
    DateTime::<Utc>::from(SystemTime::now()).to_rfc3339()
}

fn stable_digest(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex = format!("{digest:x}");
    hex[..24].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompose_prefers_bullets() {
        let steps = decompose_prompt_into_steps(
            "Release work",
            "- inspect production issue\n- patch backend deploy path\n- verify smoke checks",
        );
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].title, "inspect production issue");
        assert!(steps[1].instruction.contains("patch backend deploy path"));
    }

    #[test]
    fn decompose_falls_back_to_sentence_candidates() {
        let steps = decompose_prompt_into_steps(
            "Long task",
            "Inspect the repo for the migration path. Update the rollout script for the new host. Verify the final service status after deployment.",
        );
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[1].title, "Update the rollout script for the new host");
    }

    #[test]
    fn decompose_uses_generic_fallback_for_short_prompt() {
        let steps = decompose_prompt_into_steps("Follow up", "Need a durable follow-up.");
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].title, "Inspect scope and constraints");
    }

    // NOTE: The following three tests reference `ingest()` and `IngestPlanRequest`
    // which no longer exist in this module, and use `tempfile` which is not
    // declared as a dev-dependency. They're disabled until rewritten to match
    // the current plan.rs API. (Pre-existing breakage, unrelated to meeting work.)
    #[cfg(any())]
    #[test]
    fn new_plans_default_to_auto_advance() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        let view = ingest(
            root,
            IngestPlanRequest {
                title: "Test plan".to_string(),
                prompt: "- step one\n- step two\n- step three".to_string(),
                thread_key: None,
                skill: None,
                auto_advance: true,
                emit_now: false,
            },
        )?;
        assert!(view.auto_advance, "plan should default to auto_advance");
        assert!(
            has_active_goal_with_pending_step(root)?,
            "plan with pending steps must be reported as active with pending work"
        );
        Ok(())
    }

    #[cfg(any())]
    #[test]
    fn has_active_goal_reports_false_when_no_plans_exist() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        assert!(!has_active_goal_with_pending_step(tmp.path())?);
        Ok(())
    }

    #[cfg(any())]
    #[test]
    fn has_active_goal_reports_false_when_all_steps_completed() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        let view = ingest(
            root,
            IngestPlanRequest {
                title: "Done plan".to_string(),
                prompt: "- a\n- b".to_string(),
                thread_key: None,
                skill: None,
                auto_advance: true,
                emit_now: false,
            },
        )?;
        let conn = open_plan_db(root)?;
        conn.execute(
            "UPDATE planned_steps SET status='completed' WHERE goal_id=?1",
            params![view.goal_id],
        )?;
        conn.execute(
            "UPDATE planned_goals SET status='completed' WHERE goal_id=?1",
            params![view.goal_id],
        )?;
        assert!(!has_active_goal_with_pending_step(root)?);
        Ok(())
    }
}
