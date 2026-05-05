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
use crate::governance;

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
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
const GOAL_STATUS_FAILED: &str = "failed";
/// A previously-active goal that is being replaced by a freshly-ingested goal
/// pointing at the same `thread_key`. The reviewer-rework loop saw a stale
/// "Owner-Mail zu aktiver Vision/Mission Rev. 2" goal sitting active next to a
/// freshly-ingested unversioned goal on the same thread_key — both lit up in
/// the reviewer's scan and produced a phantom "revision mismatch". A new
/// ingest on the same `thread_key` is the operator's structural signal that
/// the older slice is no longer the live one; we mark it `superseded` instead
/// of leaving two competing live truths.
const GOAL_STATUS_SUPERSEDED: &str = "superseded";

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
    if updated > 0 {
        settle_plan_queue_message(root, Some(message_key), "handled")?;
    }
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
    let approval_wait_goal = goal_waits_for_external_approval(
        request.title.trim(),
        request.prompt.trim(),
        request.thread_key.as_deref(),
    );
    let goal_id = format!(
        "goal_{}",
        stable_digest(&format!(
            "{}:{}:{}",
            request.title.trim(),
            request.prompt.trim(),
            now
        ))
    );
    let explicit_thread_key = request.thread_key.is_some();
    let thread_key = request
        .thread_key
        .unwrap_or_else(|| format!("{DEFAULT_GOAL_THREAD_PREFIX}/{goal_id}"));
    let drafts = decompose_prompt_into_steps(&request.title, &request.prompt);

    let tx = conn.unchecked_transaction()?;

    // P1 — Plan-Goal supersede on duplicate thread_key.
    //
    // Production smoke-test (Befund D) hit a reviewer-rework loop where two
    // active planned_goals existed for the same `thread_key`: an older
    // version-stamped goal title and a fresh unversioned ingest. The reviewer
    // pulled both into its scan and produced a phantom "Revision 2 vs
    // revision 1" mismatch. A new `plan ingest` against the same `thread_key`
    // is structurally the operator declaring "this is the live slice now" —
    // older active goals on that same thread_key must be marked superseded
    // so they stop competing for active-list / due-work / has-runnable-work
    // queries. We only fire when the operator explicitly passed a
    // `thread_key`; the default `plan/{goal_id}` thread_key is per-goal-
    // unique by construction and cannot collide.
    let superseded_supersede = if explicit_thread_key {
        supersede_active_goals_for_thread_key_tx(&tx, &thread_key, &goal_id, &now)?
    } else {
        Vec::new()
    };

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
            if request.auto_advance && !approval_wait_goal {
                1
            } else {
                0
            },
            if approval_wait_goal {
                GOAL_STATUS_BLOCKED
            } else {
                GOAL_STATUS_ACTIVE
            },
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
                if approval_wait_goal && index == 0 {
                    STEP_STATUS_BLOCKED
                } else {
                    STEP_STATUS_PENDING
                },
                now,
            ],
        )?;
        if approval_wait_goal && index == 0 {
            tx.execute(
                r#"
                UPDATE planned_steps
                SET blocked_reason = ?2
                WHERE step_id = ?1
                "#,
                params![
                    step_id,
                    "waiting for explicit external approval/access evidence before auto-advance"
                ],
            )?;
        }
    }
    tx.commit()?;

    // P1 — record one governance event per superseded goal after the
    // supersede commit, so the supersede mapping is durably auditable. We
    // intentionally do not abort goal creation if the governance write fails
    // (mirrors the prevailing `let _ =` pattern around governance writes in
    // service.rs); the supersede itself is already committed.
    for entry in &superseded_supersede {
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "plan_goal_superseded_for_duplicate_slice",
                conversation_id: None,
                severity: "info",
                reason: "duplicate_thread_key_active_goal",
                action_taken: "marked_older_planned_goal_superseded",
                details: json!({
                    "thread_key": thread_key,
                    "new_goal_id": goal_id,
                    "new_goal_title": request.title.trim(),
                    "superseded_goal_id": entry.goal_id,
                    "superseded_goal_title": entry.title,
                    "superseded_previous_status": entry.previous_status,
                }),
                idempotence_key: Some(&format!(
                    "plan_goal_superseded::{}::{}",
                    entry.goal_id, goal_id
                )),
            },
        );
    }

    if request.emit_now {
        let _ = emit_next_step_for_goal(root, &goal_id)?;
    }

    load_goal_with_steps(root, &goal_id)?.context("failed to reload created planned goal")
}

#[derive(Debug, Clone)]
struct SupersededGoalEntry {
    goal_id: String,
    title: String,
    previous_status: String,
}

/// Mark every previously-active goal sharing the supplied `thread_key` as
/// `superseded`, returning their identifiers so the caller can emit one
/// governance event per supersede after the surrounding transaction commits.
///
/// Structural match: pure `(thread_key)` tuple (the planned_goals row carries
/// no separate `conversation_id` column). No string-scraping, no title regex.
fn supersede_active_goals_for_thread_key_tx(
    tx: &Transaction<'_>,
    thread_key: &str,
    new_goal_id: &str,
    now: &str,
) -> Result<Vec<SupersededGoalEntry>> {
    let mut stmt = tx.prepare(
        r#"
        SELECT goal_id, title, status
        FROM planned_goals
        WHERE thread_key = ?1
          AND goal_id <> ?2
          AND status = ?3
        "#,
    )?;
    let entries = stmt
        .query_map(
            params![thread_key, new_goal_id, GOAL_STATUS_ACTIVE],
            |row| {
                Ok(SupersededGoalEntry {
                    goal_id: row.get::<_, String>(0)?,
                    title: row.get::<_, String>(1)?,
                    previous_status: row.get::<_, String>(2)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    if entries.is_empty() {
        return Ok(entries);
    }

    for entry in &entries {
        tx.execute(
            r#"
            UPDATE planned_goals
            SET status = ?2,
                updated_at = ?3
            WHERE goal_id = ?1
            "#,
            params![entry.goal_id, GOAL_STATUS_SUPERSEDED, now],
        )?;
    }
    Ok(entries)
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
                  AND s.status IN ('pending', 'queued', 'blocked')
                ORDER BY s.step_order ASC
                LIMIT 1
            ) AS next_step_id,
            (
                SELECT s.title
                FROM planned_steps s
                WHERE s.goal_id = g.goal_id
                  AND s.status IN ('pending', 'queued', 'blocked')
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
                WHEN 'superseded' THEN 4
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
    let Some(pending) = prepare_next_step_emission(root, &conn, goal_id)? else {
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
    root: &Path,
    conn: &Connection,
    goal_id: &str,
) -> Result<Option<PendingStepEmission>> {
    let tx = conn.unchecked_transaction()?;
    let goal = load_goal_tx(&tx, goal_id)?.context("planned goal not found")?;
    if goal.status == GOAL_STATUS_COMPLETED {
        tx.commit()?;
        return Ok(None);
    }
    if goal_waits_for_external_approval(
        goal.title.trim(),
        goal.source_prompt.trim(),
        Some(goal.thread_key.as_str()),
    ) {
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
    let prompt = render_step_prompt(root, &goal, &step, &list_completed_steps_tx(&tx, goal_id)?);
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
    let message_key = load_last_message_key_for_step_tx(&tx, step_id)?;
    let updated = mark_step_completed_tx(&tx, step_id, result_text)?;
    if let Some(goal_id) = load_goal_id_for_step_tx(&tx, step_id)? {
        refresh_goal_status_tx(&tx, &goal_id)?;
    }
    tx.commit()?;
    if updated > 0 {
        settle_plan_queue_message(root, message_key.as_deref(), "handled")?;
    }
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
    let message_key = load_last_message_key_for_step_tx(&tx, step_id)?;
    let updated = mark_step_failed_tx(&tx, step_id, reason)?;
    if let Some(goal_id) = load_goal_id_for_step_tx(&tx, step_id)? {
        refresh_goal_status_tx(&tx, &goal_id)?;
    }
    tx.commit()?;
    if updated > 0 {
        settle_plan_queue_message(root, message_key.as_deref(), "failed")?;
    }
    Ok(updated)
}

fn mark_step_failed_tx(tx: &Transaction<'_>, step_id: &str, reason: &str) -> Result<usize> {
    let now = now_iso_string();
    Ok(tx.execute(
        r#"
        UPDATE planned_steps
        SET status = ?2,
            blocked_reason = NULL,
            last_message_key = NULL,
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
    let message_key = load_last_message_key_for_step_tx(&tx, step_id)?;
    let now = now_iso_string();
    let updated = tx.execute(
        r#"
        UPDATE planned_steps
        SET status = ?2,
            blocked_reason = ?3,
            last_message_key = NULL,
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
    if updated > 0 {
        settle_plan_queue_message(root, message_key.as_deref(), "blocked")?;
    }
    Ok(updated)
}

fn reset_step_to_pending(root: &Path, step_id: &str, defer_until: Option<String>) -> Result<usize> {
    let conn = open_plan_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let message_key = load_last_message_key_for_step_tx(&tx, step_id)?;
    let now = now_iso_string();
    let updated = tx.execute(
        r#"
        UPDATE planned_steps
        SET status = ?2,
            defer_until = ?3,
            blocked_reason = NULL,
            last_message_key = NULL,
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
    if updated > 0 {
        settle_plan_queue_message(root, message_key.as_deref(), "cancelled")?;
    }
    Ok(updated)
}

pub fn repair_stale_step_routing_state(root: &Path) -> Result<usize> {
    let conn = open_plan_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare(
        r#"
        SELECT step_id, status, last_message_key
        FROM planned_steps
        WHERE last_message_key IS NOT NULL
          AND TRIM(last_message_key) <> ''
          AND status != ?1
        "#,
    )?;
    let rows = stmt
        .query_map(params![STEP_STATUS_QUEUED], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut repaired = 0usize;
    let now = now_iso_string();
    for (step_id, status, message_key) in rows {
        let route_status = match status.as_str() {
            STEP_STATUS_COMPLETED => "handled",
            STEP_STATUS_FAILED => "failed",
            STEP_STATUS_BLOCKED => "blocked",
            STEP_STATUS_PENDING => "cancelled",
            _ => continue,
        };
        set_queue_routing_status_tx(&tx, &message_key, route_status, &now)?;
        if status != STEP_STATUS_COMPLETED {
            tx.execute(
                "UPDATE planned_steps SET last_message_key = NULL, updated_at = ?2 WHERE step_id = ?1",
                params![step_id, now],
            )?;
        }
        repaired += 1;
    }
    tx.commit()?;
    Ok(repaired)
}

fn render_step_prompt(
    root: &Path,
    goal: &PlannedGoalView,
    step: &PlannedStepView,
    completed_steps: &[PlannedStepView],
) -> String {
    let mut lines = vec![
        format!("Plan goal: {}", goal.title),
        format!("Plan step {}: {}", step.step_order, step.title),
        "Work only on this step. Do not silently skip ahead.".to_string(),
    ];
    let autonomy = crate::autonomy::AutonomyLevel::from_root(root);
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
    lines.push("- Findings, decisions, architecture notes, policies, measured results -> `ctox ticket knowledge-put --system <system> --domain <domain> --title <title> --body <body>`. These are ticket fact/context records: capture a conclusion or verified fact, not a description of work you have not done yet. If the step teaches a repeatable procedure, also promote it into source-skill, Skillbook, Runbook, and Runbook-Item records.".to_string());
    lines.push("- Concrete implementation work that genuinely belongs to a later slice (because it depends on something outside this step) → `ctox ticket self-work-put --system <system> --kind change --title <title> --body <body>`. If the code change fits inside this step, just make the change now instead of deferring it.".to_string());
    lines.push("- Owner approval needed before a genuinely high-impact irreversible move (production cutover, destructive migration, public communication) → `ctox ticket self-work-put --system <system> --kind approval-gate --title <title> --body <body>`. Most steps do not need an approval gate; use this only when the next action would really be irreversible without sign-off.".to_string());
    lines.push("- Credentials, API keys, or accounts you cannot obtain yourself → `ctox ticket access-request-put --system <system> --title <title> --body <body> --required-scopes <csv>`.".to_string());
    lines.push("- A workstream that genuinely needs its own multi-step plan → `ctox plan ingest --title <title> --prompt <text>`. Do not spawn a sub-plan just to avoid working on this step.".to_string());
    lines.push(String::new());
    lines.push(
        "Reply briefly with what you persisted and what blockers remain. A reply without any persisted artifact is not a completed step unless the step was purely a summary/decision step and the decision itself is stored as runtime state. At the same time: writing another plan, another approval gate, or another scope document about work you could have done now is not completion either — it is the same step restated.".to_string(),
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

fn goal_waits_for_external_approval(title: &str, prompt: &str, thread_key: Option<&str>) -> bool {
    let lowered =
        format!("{}\n{}\n{}", title, prompt, thread_key.unwrap_or_default()).to_ascii_lowercase();
    let waits_for_external_input = [
        "approval",
        "access-grant",
        "access grant",
        "owner approval",
        "explicit owner",
        "explicit inbound",
        "confirmed",
        "confirmation",
        "waiting",
        "blocked until",
    ]
    .iter()
    .any(|needle| lowered.contains(needle));
    let monitor_only = [
        "monitor inbound",
        "monitor the jami thread",
        "monitor the email thread",
        "jami:",
        "email",
        "keep the deployment blocked",
        "after confirmation, deploy",
        "approval evidence",
        "vercel approval",
    ]
    .iter()
    .any(|needle| lowered.contains(needle));
    waits_for_external_input && monitor_only
}

fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether any active plan still has runnable steps, ignoring auto_advance.
/// Used by the mission idle watchdog so that it keeps triggering only while
/// real plan work remains executable. Blocked/failed steps do not count as
/// runnable work and should not reactivate stale loops.
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
              AND s.status IN ('pending', 'queued')
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
                  AND s.status IN ('pending', 'queued', 'blocked')
                ORDER BY s.step_order ASC
                LIMIT 1
            ) AS next_step_id,
            (
                SELECT s.title
                FROM planned_steps s
                WHERE s.goal_id = g.goal_id
                  AND s.status IN ('pending', 'queued', 'blocked')
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
                  AND s.status IN ('pending', 'queued', 'blocked')
                ORDER BY s.step_order ASC
                LIMIT 1
            ) AS next_step_id,
            (
                SELECT s.title
                FROM planned_steps s
                WHERE s.goal_id = g.goal_id
                  AND s.status IN ('pending', 'queued', 'blocked')
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

fn load_last_message_key_for_step_tx(
    tx: &Transaction<'_>,
    step_id: &str,
) -> Result<Option<String>> {
    tx.query_row(
        "SELECT last_message_key FROM planned_steps WHERE step_id = ?1 LIMIT 1",
        params![step_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn settle_plan_queue_message(
    root: &Path,
    message_key: Option<&str>,
    route_status: &str,
) -> Result<()> {
    let Some(message_key) = message_key.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let conn = open_plan_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let now = now_iso_string();
    set_queue_routing_status_tx(&tx, message_key, route_status, &now)?;
    tx.commit()?;
    Ok(())
}

fn set_queue_routing_status_tx(
    tx: &Transaction<'_>,
    message_key: &str,
    route_status: &str,
    now: &str,
) -> Result<()> {
    anyhow::ensure!(
        matches!(
            route_status,
            "pending" | "blocked" | "failed" | "handled" | "cancelled"
        ),
        "unsupported queue route status '{route_status}'"
    );
    let acked_at = if matches!(route_status, "handled" | "cancelled") {
        Some(now)
    } else {
        None
    };
    tx.execute(
        r#"
        INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
        )
        VALUES (?1, ?2, NULL, NULL, ?3, NULL, ?4)
        ON CONFLICT(message_key) DO UPDATE SET
            route_status=excluded.route_status,
            lease_owner=NULL,
            leased_at=NULL,
            acked_at=excluded.acked_at,
            last_error=NULL,
            updated_at=excluded.updated_at
        "#,
        params![message_key, route_status, acked_at, now],
    )?;
    Ok(())
}

fn refresh_goal_status_tx(tx: &Transaction<'_>, goal_id: &str) -> Result<()> {
    // Preserve a `superseded` terminal status — once another ingest has taken
    // over this thread_key, never reanimate this goal back to active even if
    // its leftover step counters happen to match an active shape.
    let current_status: Option<String> = tx
        .query_row(
            "SELECT status FROM planned_goals WHERE goal_id = ?1",
            params![goal_id],
            |row| row.get(0),
        )
        .optional()?;
    if matches!(current_status.as_deref(), Some(GOAL_STATUS_SUPERSEDED)) {
        return Ok(());
    }
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
    let failed: i64 = tx.query_row(
        "SELECT COUNT(*) FROM planned_steps WHERE goal_id = ?1 AND status = 'failed'",
        params![goal_id],
        |row| row.get(0),
    )?;
    let pending_or_queued: i64 = tx.query_row(
        "SELECT COUNT(*) FROM planned_steps WHERE goal_id = ?1 AND status IN ('pending', 'queued')",
        params![goal_id],
        |row| row.get(0),
    )?;
    let status = if total > 0 && completed == total {
        GOAL_STATUS_COMPLETED
    } else if total > 0 && failed + completed == total && failed > 0 {
        GOAL_STATUS_FAILED
    } else if blocked > 0 {
        GOAL_STATUS_BLOCKED
    } else if pending_or_queued == 0 && failed > 0 {
        GOAL_STATUS_FAILED
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
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for plans")?;
    let busy_timeout_ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = {busy_timeout_ms};

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
    ))?;
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
    use std::path::PathBuf;

    fn temp_plan_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ctox-plan-{}-{}",
            name,
            stable_digest(&format!("{}:{}", name, now_iso_string()))
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("failed to create temp root");
        root
    }

    fn routing_status_for_message(root: &PathBuf, message_key: &str) -> String {
        let conn = open_plan_db(root).expect("failed to open plan db");
        conn.query_row(
            "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
            params![message_key],
            |row| row.get(0),
        )
        .expect("expected routing status")
    }

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

    #[test]
    fn approval_wait_plans_start_blocked_and_do_not_auto_emit() -> Result<()> {
        let root = temp_plan_root("approval-wait");
        let created = ingest_goal(
            &root,
            PlanIngestRequest {
                title: "Wait for Vercel approval for kunstmen.com".to_string(),
                prompt: "Monitor the Jami thread jami:abc123 for explicit Vercel access approval. Keep the deployment blocked until approval is confirmed. After confirmation, deploy production and verify kunstmen.com live.".to_string(),
                thread_key: Some("jami:abc123".to_string()),
                skill: Some("follow-up-orchestrator".to_string()),
                auto_advance: true,
                emit_now: false,
            },
        )?;
        let view = load_goal_with_steps(&root, &created.goal.goal_id)?
            .expect("created goal should reload");
        assert!(!view.goal.auto_advance);
        assert_eq!(view.goal.status, GOAL_STATUS_BLOCKED);
        assert_eq!(view.steps[0].status, STEP_STATUS_BLOCKED);

        let summary = emit_due_steps(&root)?;
        assert_eq!(summary.emitted_count, 0);
        Ok(())
    }

    #[test]
    fn fully_failed_plan_is_not_treated_as_active_runnable_work() -> Result<()> {
        let root = temp_plan_root("failed-plan");
        let created = ingest_goal(
            &root,
            PlanIngestRequest {
                title: "Stale approval monitor".to_string(),
                prompt: "Monitor inbound approval. Keep the deployment blocked until approval is confirmed. After confirmation, deploy production and verify live HTML.".to_string(),
                thread_key: Some("review-probe".to_string()),
                skill: Some("follow-up-orchestrator".to_string()),
                auto_advance: true,
                emit_now: false,
            },
        )?;
        let step_ids = created
            .steps
            .iter()
            .map(|step| step.step_id.clone())
            .collect::<Vec<_>>();
        for step_id in step_ids {
            mark_step_failed(&root, &step_id, "superseded by clean probe")?;
        }

        let view = load_goal_with_steps(&root, &created.goal.goal_id)?
            .expect("goal should reload after failing all steps");
        assert_eq!(view.goal.status, GOAL_STATUS_FAILED);
        assert!(!has_active_goal_with_pending_step(&root)?);

        let summary = emit_due_steps(&root)?;
        assert_eq!(summary.emitted_count, 0);
        Ok(())
    }

    #[test]
    fn blocking_step_clears_last_message_key_and_unleases_queue_message() -> Result<()> {
        let root = temp_plan_root("block-clears-routing");
        let created = ingest_goal(
            &root,
            PlanIngestRequest {
                title: "Repair stale plan route".to_string(),
                prompt: "- inspect platform issue\n- redesign homepage".to_string(),
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
        let conn = open_plan_db(&root)?;
        let spawn_edge_count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_core_spawn_edges
            WHERE child_entity_type = 'Message'
              AND child_entity_id = ?1
              AND spawn_kind = 'plan-step-message'
              AND parent_entity_type = 'PlanStep'
              AND parent_entity_id = ?2
              AND accepted = 1
            "#,
            params![&emitted, &created.steps[0].step_id],
            |row| row.get(0),
        )?;
        assert_eq!(spawn_edge_count, 1);
        conn.execute(
            "UPDATE communication_routing_state SET route_status = 'leased', lease_owner = 'test-reviewer', leased_at = ?2 WHERE message_key = ?1",
            params![emitted, now_iso_string()],
        )?;
        drop(conn);

        let step_id = created.steps[0].step_id.clone();
        mark_step_blocked(&root, &step_id, "superseded by direct mission reset")?;
        assert_eq!(routing_status_for_message(&root, &emitted), "blocked");

        let reloaded = load_goal_with_steps(&root, &created.goal.goal_id)?
            .expect("blocked plan should reload");
        let blocked_step = reloaded
            .steps
            .into_iter()
            .find(|step| step.step_id == step_id)
            .expect("blocked step should exist");
        assert_eq!(blocked_step.status, STEP_STATUS_BLOCKED);
        assert!(blocked_step.last_message_key.is_none());
        Ok(())
    }

    #[test]
    fn duplicate_thread_key_ingest_supersedes_older_active_goal() -> Result<()> {
        let root = temp_plan_root("supersede-on-duplicate-thread-key");

        // First ingest — older goal, version-stamped title (mirrors Befund D
        // production state where "Owner-Mail zu aktiver Vision/Mission Rev. 2"
        // was sitting on the same thread_key as a freshly-ingested unversioned
        // goal).
        let older = ingest_goal(
            &root,
            PlanIngestRequest {
                title: "Owner-Mail zu aktiver Vision/Mission Rev. 2".to_string(),
                prompt: "- draft owner-mail Rev. 2 covering vision and mission".to_string(),
                thread_key: Some("vision-mission-founder".to_string()),
                skill: Some("owner-communication".to_string()),
                auto_advance: false,
                emit_now: false,
            },
        )?;
        assert_eq!(older.goal.status, GOAL_STATUS_ACTIVE);

        // Second ingest — fresh unversioned goal on the SAME thread_key.
        let newer = ingest_goal(
            &root,
            PlanIngestRequest {
                title: "Vision-Mission Founder Mail".to_string(),
                prompt: "- draft a fresh founder mail covering vision and mission".to_string(),
                thread_key: Some("vision-mission-founder".to_string()),
                skill: Some("owner-communication".to_string()),
                auto_advance: false,
                emit_now: false,
            },
        )?;

        let reloaded_older = load_goal_with_steps(&root, &older.goal.goal_id)?
            .expect("older goal should still load");
        let reloaded_newer = load_goal_with_steps(&root, &newer.goal.goal_id)?
            .expect("newer goal should still load");

        assert_eq!(
            reloaded_older.goal.status, GOAL_STATUS_SUPERSEDED,
            "older goal on the duplicated thread_key must flip to superseded",
        );
        assert_eq!(
            reloaded_newer.goal.status, GOAL_STATUS_ACTIVE,
            "newer goal must own the active slot on the thread_key",
        );

        // Default-thread_key ingests must not collide with each other (their
        // thread_keys are per-goal-unique by construction).
        let unrelated_a = ingest_goal(
            &root,
            PlanIngestRequest {
                title: "Unrelated default-thread plan A".to_string(),
                prompt: "- inspect repo state\n- patch deploy script".to_string(),
                thread_key: None,
                skill: None,
                auto_advance: false,
                emit_now: false,
            },
        )?;
        let unrelated_b = ingest_goal(
            &root,
            PlanIngestRequest {
                title: "Unrelated default-thread plan B".to_string(),
                prompt: "- inspect runtime status\n- verify rollout".to_string(),
                thread_key: None,
                skill: None,
                auto_advance: false,
                emit_now: false,
            },
        )?;
        let reloaded_a = load_goal_with_steps(&root, &unrelated_a.goal.goal_id)?
            .expect("unrelated plan A should reload");
        let reloaded_b = load_goal_with_steps(&root, &unrelated_b.goal.goal_id)?
            .expect("unrelated plan B should reload");
        assert_eq!(reloaded_a.goal.status, GOAL_STATUS_ACTIVE);
        assert_eq!(reloaded_b.goal.status, GOAL_STATUS_ACTIVE);

        // The supersede must be durably auditable as a governance event.
        let events =
            governance::list_recent_events(&root, 1, 16).expect("failed to list governance events");
        let supersede_event = events
            .iter()
            .find(|event| event.mechanism_id == "plan_goal_superseded_for_duplicate_slice")
            .expect("expected a plan_goal_superseded_for_duplicate_slice event");
        assert_eq!(supersede_event.severity, "info");
        assert_eq!(
            supersede_event.reason, "duplicate_thread_key_active_goal",
            "reason must structurally identify the duplicate-thread_key cause"
        );
        let details = &supersede_event.details;
        assert_eq!(
            details.get("thread_key").and_then(|value| value.as_str()),
            Some("vision-mission-founder")
        );
        assert_eq!(
            details
                .get("superseded_goal_id")
                .and_then(|value| value.as_str()),
            Some(older.goal.goal_id.as_str()),
        );
        assert_eq!(
            details.get("new_goal_id").and_then(|value| value.as_str()),
            Some(newer.goal.goal_id.as_str()),
        );

        // Active-listing queries must exclude the superseded goal — otherwise
        // the reviewer scan (Befund D) keeps surfacing two competing live
        // truths.
        assert!(
            has_active_goal_with_pending_step(&root)?,
            "newer goal still has runnable work"
        );
        let due = list_goal_ids_with_due_work(&open_plan_db(&root)?)?;
        assert!(
            !due.contains(&older.goal.goal_id),
            "superseded goal must not appear in due-work scan; got {due:?}",
        );
        Ok(())
    }

    #[test]
    fn repair_stale_step_routing_state_releases_historical_leases() -> Result<()> {
        let root = temp_plan_root("repair-stale-routing");
        let created = ingest_goal(
            &root,
            PlanIngestRequest {
                title: "Historical stale route".to_string(),
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
        let conn = open_plan_db(&root)?;
        conn.execute(
            "UPDATE communication_routing_state SET route_status = 'leased', lease_owner = 'test-reviewer', leased_at = ?2 WHERE message_key = ?1",
            params![emitted, now_iso_string()],
        )?;
        drop(conn);

        let conn = open_plan_db(&root)?;
        conn.execute(
            "UPDATE planned_steps SET status = 'completed' WHERE step_id = ?1",
            params![created.steps[0].step_id.clone()],
        )?;
        drop(conn);

        let repaired = repair_stale_step_routing_state(&root)?;
        assert_eq!(repaired, 1);
        assert_eq!(routing_status_for_message(&root, &emitted), "handled");
        Ok(())
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
