// Origin: CTOX
// License: AGPL-3.0-only

//! The native calendar admits one preparation on the registered supervisor.
//! A queued preparation is not proof of a ready deck or playable narration.
use super::super::{threads, workjet_identity, workjet_jour_fixe_contract as wire};
use super::weekly_reports::{request, ReportRoute};
use super::*;
use crate::mission::schedule::{self, ScheduledTaskView};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use wire::WireValidate;

pub(super) const PREFIX: &str = "workjet-jour-fixe-prepare:";
const MARKER: &str = "CTOX_WORKJET_JOUR_FIXE_PREPARE:";
pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_jour_fixe_meetings (
 meeting_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, owner_user_id TEXT NOT NULL,
 scheduled_at_ms INTEGER NOT NULL, metadata_json TEXT NOT NULL, preparation_task_id TEXT
);
CREATE INDEX IF NOT EXISTS workjet_jour_fixe_by_project
 ON workjet_jour_fixe_meetings(project_id, scheduled_at_ms DESC);";

fn preparation_request(
    route: &ReportRoute,
) -> anyhow::Result<(schedule::ScheduleEnsureRequest, schedule::ScheduleCalendar)> {
    let (mut task, mut calendar) = request(route)?;
    task.name = format!(
        "{PREFIX}{}",
        stable_id("project", &[&route.owner_user_id, &route.project_id])
    );
    task.prompt = format!("{MARKER}{}", serde_json::to_string(route)?);
    calendar.lead_minutes = 120;
    Ok((task, calendar))
}

pub(super) fn ensure_schedule(root: &Path, route: &ReportRoute) -> anyhow::Result<String> {
    let (task, calendar) = preparation_request(route)?;
    let name = task.name.clone();
    schedule::ensure_task_with_calendar_preserving_pause(root, task, calendar)?;
    Ok(name)
}

pub(super) fn emit(
    root: &Path,
    task: &ScheduledTaskView,
    scheduled_for: &str,
) -> anyhow::Result<Option<(String, String)>> {
    if !task.name.starts_with(PREFIX) {
        return Ok(None);
    }
    let route: ReportRoute = serde_json::from_str(
        task.prompt
            .strip_prefix(MARKER)
            .context("managed preparation route missing")?,
    )?;
    let binding = supervisor_turns::binding(
        root,
        &route.owner_user_id,
        &route.project_id,
        &route.thread_id,
        true,
    )?;
    let session = super::super::store::active_domain_recovery_session(root, &route.owner_user_id)?;
    ensure!(
        super::super::store::module_policy_decision(
            root,
            &session,
            super::super::policy::BusinessOsPermission::CtoxTaskCreate,
            "ctox"
        )?
        .allowed,
        "preparation owner may not create CTOX tasks"
    );
    let (expected, calendar) = preparation_request(&route)?;
    ensure!(
        task.name == expected.name
            && task.thread_key == binding.thread_key
            && task.cron_expr == expected.cron_expr
            && task.calendar == calendar
            && task.skill.is_none(),
        "managed preparation conflicts with project binding"
    );
    let mut conn = open_store(root)?;
    let project = owned_project(&conn, &route.project_id, &route.owner_user_id, true)?;
    ensure!(
        project.get("jour_fixe") == Some(&route.jour_fixe),
        "preparation configuration changed"
    );
    let prepare_at = chrono::DateTime::parse_from_rfc3339(scheduled_for)?.timestamp_millis();
    let scheduled_at = prepare_at
        .checked_add(120 * 60 * 1000)
        .context("appointment time overflow")?;
    let meeting_id = stable_id("workjet_meeting", &[&task.task_id, scheduled_for]);
    let operation = stable_id("workjet_meeting_prepare", &[&task.task_id, scheduled_for]);
    let meeting = wire::Meeting {
        id: meeting_id.clone(),
        project_id: route.project_id.clone(),
        owner_user_id: route.owner_user_id.clone(),
        supervisor: wire::SupervisorRef {
            workjet_thread_id: binding.thread_id.clone(),
            ctox_thread_key: binding.thread_key.clone(),
        },
        scheduled_at_ms: scheduled_at,
        prepare_at_ms: prepare_at,
        timezone: calendar.timezone.clone(),
        state: wire::MeetingState::Planned,
        revision: 0,
        deck_revision: 0,
        previous_goal: None,
        slides: vec![],
        comments: vec![],
        transcript: vec![],
        todos: None,
        error: None,
    };
    meeting.validate().map_err(anyhow::Error::msg)?;
    // Metadata is native-only. Dispatch runs outside its write transaction;
    // the existing command receipt makes a crash/retry admit the same turn.
    conn.execute_batch(SCHEMA)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("INSERT INTO workjet_jour_fixe_meetings(meeting_id,project_id,owner_user_id,scheduled_at_ms,metadata_json)
      VALUES (?1,?2,?3,?4,?5) ON CONFLICT(meeting_id) DO NOTHING",
      params![meeting_id, route.project_id, route.owner_user_id, scheduled_at, serde_json::to_string(&meeting)?])?;
    let raw: String = tx.query_row(
        "SELECT metadata_json FROM workjet_jour_fixe_meetings WHERE meeting_id=?1",
        [&meeting_id],
        |r| r.get(0),
    )?;
    let existing: wire::Meeting = serde_json::from_str(&raw)?;
    ensure!(
        existing.project_id == meeting.project_id
            && existing.owner_user_id == meeting.owner_user_id
            && existing.supervisor.workjet_thread_id == binding.thread_id
            && existing.supervisor.ctox_thread_key == binding.thread_key
            && existing.scheduled_at_ms == scheduled_at
            && existing.prepare_at_ms == prepare_at
            && existing.timezone == calendar.timezone,
        "preparation occurrence conflicts with stored meeting"
    );
    tx.commit()?;
    drop(conn);
    let accepted = super::super::store::accept_rxdb_business_command_with_origin(
        root,
        json!({
            "id":operation,"module":"ctox","command_type":"ctox.workjet.project.supervisor.turn.submit","record_id":route.project_id,
            "payload":{"project_id":route.project_id,"thread_id":route.thread_id,"goal":format!(
              "JourFix preparation for meeting {meeting_id}, appointment {scheduled_at} ms UTC.\n\n{}", include_str!("../../../skills/system/mission_orchestration/jour-fix/SKILL.md"))},
            "client_context":{"actor":threads::actor_payload(&session),"source":"native-workjet-jour-fixe-schedule"}
        }),
        super::super::store::CommandOrigin::TrustedLocal,
    )?;
    ensure!(
        accepted["status"] == "completed"
            && accepted["result"]["binding"] == serde_json::to_value(&binding)?,
        "preparation was not admitted to its registered supervisor: {}",
        accepted["error_message"]
    );
    let key = accepted["result"]["turn"]["task_id"]
        .as_str()
        .context("preparation has no native queue receipt")?
        .to_owned();
    let conn = open_store(root)?;
    // Do not replace metadata or a future deck/review on occurrence replay.
    conn.execute(
        "UPDATE workjet_jour_fixe_meetings SET preparation_task_id=?2
       WHERE meeting_id=?1 AND preparation_task_id IS NULL",
        params![meeting_id, key],
    )?;
    let saved: String = conn.query_row(
        "SELECT preparation_task_id FROM workjet_jour_fixe_meetings WHERE meeting_id=?1",
        [&meeting_id],
        |r| r.get(0),
    )?;
    ensure!(saved == key, "preparation has a conflicting queue receipt");
    Ok(Some((key, "emitted".to_owned())))
}

pub(in crate::business_os) fn read(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
) -> anyhow::Result<Value> {
    let mut payload = command.payload.clone();
    let object = payload
        .as_object_mut()
        .context("meeting read must be an object")?;
    if let Some(channel) = object.remove("inbound_channel") {
        let text = channel.as_str().context("inbound_channel must be text")?;
        ensure!(
            !text.trim().is_empty() && text.chars().count() <= 256,
            "invalid inbound_channel"
        );
    }
    let query: wire::ReadMeetingRequest = serde_json::from_value(payload)?;
    query.validate().map_err(anyhow::Error::msg)?;
    let conn = open_store(root)?;
    let owner = workjet_identity::owner_from_connection(&conn, actor)?;
    let project = owned_project(&conn, &query.project_id, &owner, true)?;
    ensure!(
        project["id"] == query.project_id,
        "project id must be canonical"
    );
    ensure!(
        command
            .record_id
            .as_deref()
            .is_none_or(|id| id == query.project_id),
        "meeting read routing conflicts with project"
    );
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_jour_fixe_meetings')", [], |r| r.get(0))?;
    if !exists {
        ensure!(
            query.meeting_id.is_none(),
            "meeting unavailable to this project owner"
        );
        return Ok(json!({"ok":true,"meeting":null}));
    }
    let raw: Option<(String, Option<String>)> = if let Some(id) = &query.meeting_id {
        ensure!(
            !id.trim().is_empty() && id.trim() == id,
            "meeting id must be canonical"
        );
        conn.query_row("SELECT metadata_json,preparation_task_id FROM workjet_jour_fixe_meetings WHERE meeting_id=?1 AND project_id=?2 AND owner_user_id=?3", params![id,query.project_id,owner], |r| Ok((r.get(0)?,r.get(1)?))).optional()?
    } else {
        conn.query_row("SELECT metadata_json,preparation_task_id FROM workjet_jour_fixe_meetings WHERE project_id=?1 AND owner_user_id=?2 ORDER BY scheduled_at_ms DESC,meeting_id DESC LIMIT 1", params![query.project_id,owner], |r| Ok((r.get(0)?,r.get(1)?))).optional()?
    };
    let Some((raw, key)) = raw else {
        ensure!(
            query.meeting_id.is_none(),
            "meeting unavailable to this project owner"
        );
        return Ok(json!({"ok":true,"meeting":null}));
    };
    let meeting: wire::Meeting = serde_json::from_str(&raw)?;
    meeting.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        meeting.project_id == query.project_id && meeting.owner_user_id == owner,
        "stored meeting ownership conflicts"
    );
    let binding = supervisor_turns::binding(
        root,
        &owner,
        &query.project_id,
        &meeting.supervisor.workjet_thread_id,
        true,
    )?;
    ensure!(
        meeting.supervisor.ctox_thread_key == binding.thread_key,
        "meeting execution identity conflicts"
    );
    Ok(json!({"ok":true,"meeting":meeting,"preparation_task_id":key}))
}
