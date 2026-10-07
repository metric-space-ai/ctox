// Origin: CTOX
// License: AGPL-3.0-only

//! Server schedules use the native supervisor registry, never a client timer.
use super::super::{policy::BusinessOsPermission, store, threads};
use super::*;
use crate::mission::schedule::{self, ScheduleCalendar, ScheduleEnsureRequest, ScheduledTaskView};
use serde::Serialize;
use std::collections::BTreeSet;

const PREFIX: &str = "workjet-weekly-report:";
const MARKER: &str = "CTOX_WORKJET_WEEKLY_REPORT:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ReportRoute {
    pub(super) project_id: String,
    pub(super) owner_user_id: String,
    pub(super) thread_id: String,
    pub(super) jour_fixe: Value,
}

fn name(route: &ReportRoute) -> String {
    format!(
        "{PREFIX}{}",
        stable_id("project", &[&route.owner_user_id, &route.project_id])
    )
}

pub(super) fn request(
    route: &ReportRoute,
) -> anyhow::Result<(ScheduleEnsureRequest, ScheduleCalendar)> {
    let weekday = route.jour_fixe["weekday"]
        .as_u64()
        .context("report has no weekday")?;
    ensure!((1..=7).contains(&weekday), "weekday must be ISO 1..7");
    let time = route.jour_fixe["time"]
        .as_str()
        .context("report has no time")?;
    let parsed = chrono::NaiveTime::parse_from_str(time, "%H:%M")?;
    ensure!(time.len() == 5, "time must be HH:mm");
    use chrono::Timelike;
    let timezone = route.jour_fixe["timezone"]
        .as_str()
        .unwrap_or("Europe/Berlin");
    let _: chrono_tz::Tz = timezone.parse().context("invalid report timezone")?;
    Ok((
        ScheduleEnsureRequest {
            name: name(route),
            cron_expr: format!("{} {} * * {}", parsed.minute(), parsed.hour(), weekday % 7),
            prompt: format!("{MARKER}{}", serde_json::to_string(route)?),
            thread_key: format!("business-os/threads/{}", route.thread_id),
            skill: None,
        },
        ScheduleCalendar {
            timezone: timezone.to_owned(),
            lead_minutes: 0,
        },
    ))
}

/// Reconcile committed configuration before a scheduler tick. Unchanged due
/// times and operator pauses survive. Archive/clear/revocation disables a task;
/// re-enabling it requires the explicit native schedule resume command.
pub(crate) fn reconcile_project_reports(root: &Path) -> anyhow::Result<()> {
    if !store::business_os_store_path(root).exists() {
        return Ok(());
    }
    let conn = open_store(root)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_supervisor_bindings')",
        [], |row| row.get(0))?;
    let mut routes = Vec::new();
    if exists {
        let mut stmt = conn.prepare("SELECT project_id, owner_user_id, thread_id FROM workjet_supervisor_bindings ORDER BY project_id")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (project_id, owner, thread_id) = row?;
            let Some(project) = outbound_load_record(&conn, "workjet_projects", &project_id)?
            else {
                continue;
            };
            if project["status"] != "active"
                || project["is_deleted"] == true
                || project["_deleted"] == true
                || project["owner_user_id"] != owner
            {
                continue;
            }
            let Some(meeting) = project.get("jour_fixe").filter(|v| !v.is_null()) else {
                continue;
            };
            routes.push(ReportRoute {
                project_id,
                owner_user_id: owner,
                thread_id,
                jour_fixe: meeting.clone(),
            });
        }
    }
    drop(conn);
    let mut desired = BTreeSet::new();
    let mut desired_preparations = BTreeSet::new();
    for route in routes {
        // Stale/revoked native authority never becomes a synthetic owner.
        let valid = (|| -> anyhow::Result<_> {
            supervisor_turns::binding(
                root,
                &route.owner_user_id,
                &route.project_id,
                &route.thread_id,
                true,
            )?;
            let session = store::active_domain_recovery_session(root, &route.owner_user_id)?;
            ensure!(
                store::module_policy_decision(
                    root,
                    &session,
                    BusinessOsPermission::CtoxTaskCreate,
                    "ctox"
                )?
                .allowed,
                "report owner may not create CTOX tasks"
            );
            request(&route)
        })();
        match valid {
            Ok((request, calendar)) => {
                desired.insert(request.name.clone());
                schedule::ensure_task_with_calendar_preserving_pause(root, request, calendar)?;
                desired_preparations
                    .insert(super::jour_fixe_preparation::ensure_schedule(root, &route)?);
            }
            // A storage failure is unknown authority, not a durable revocation.
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.is::<rusqlite::Error>() || cause.is::<std::io::Error>()) =>
            {
                return Err(error)
            }
            Err(_) => {}
        }
    }
    for task in schedule::list_tasks(root)? {
        if task.name.starts_with(PREFIX) && task.enabled && !desired.contains(&task.name) {
            schedule::set_task_enabled(root, &task.task_id, false)?;
        }
        if task.name.starts_with(super::jour_fixe_preparation::PREFIX)
            && task.enabled
            && !desired_preparations.contains(&task.name)
        {
            schedule::set_task_enabled(root, &task.task_id, false)?;
        }
    }
    Ok(())
}

/// Runs before opening the schedule writer transaction: the native command
/// plane writes that same Core database. Its durable receipt makes a replay of
/// this occurrence return the same message/turn on the registered CodeThread.
pub(crate) fn emit_project_report(
    root: &Path,
    task: &ScheduledTaskView,
    scheduled_for: &str,
) -> anyhow::Result<Option<(String, String)>> {
    if !task.name.starts_with(PREFIX) {
        return super::jour_fixe_preparation::emit(root, task, scheduled_for);
    }
    let route: ReportRoute = serde_json::from_str(
        task.prompt
            .strip_prefix(MARKER)
            .context("managed report route missing")?,
    )?;
    let binding = supervisor_turns::binding(
        root,
        &route.owner_user_id,
        &route.project_id,
        &route.thread_id,
        true,
    )?;
    let conn = open_store(root)?;
    let project = outbound_load_record(&conn, "workjet_projects", &route.project_id)?
        .context("project disappeared")?;
    ensure!(
        project.get("jour_fixe") == Some(&route.jour_fixe),
        "report configuration changed"
    );
    let (expected, calendar) = request(&route)?;
    ensure!(
        task.name == expected.name
            && task.thread_key == binding.thread_key
            && task.cron_expr == expected.cron_expr
            && task.calendar == calendar
            && task.skill.is_none(),
        "managed report conflicts with native project binding"
    );
    drop(conn);
    let session = store::active_domain_recovery_session(root, &route.owner_user_id)?;
    let operation = stable_id("workjet_weekly_report", &[&task.task_id, scheduled_for]);
    let accepted = store::accept_rxdb_business_command_with_origin(
        root,
        json!({
            "id": operation, "module": "ctox", "command_type": "ctox.workjet.project.supervisor.turn.submit",
            "record_id": route.project_id,
            "payload": {"project_id": route.project_id, "thread_id": route.thread_id,
                "goal": format!("Write this project's weekly report in this supervisor chat, as of {scheduled_for}. Report progress against the project goal, merged PRs with evidence, and open decisions that need the owner. Mark missing evidence explicitly; do not invent completion. Use read-only evidence. This report does not authorize deployments, external messages or new delegated work.")},
            "client_context": {"actor": threads::actor_payload(&session), "source": "native-workjet-weekly-schedule"}
        }),
        store::CommandOrigin::TrustedLocal,
    )?;
    ensure!(
        accepted["status"] == "completed"
            && accepted["result"]["binding"] == serde_json::to_value(&binding)?,
        "report not admitted to registered supervisor: {}",
        accepted["error_message"]
    );
    let key = accepted["result"]["turn"]["task_id"]
        .as_str()
        .context("report has no native queue receipt")?
        .to_owned();
    Ok(Some((key, "emitted".to_owned())))
}
