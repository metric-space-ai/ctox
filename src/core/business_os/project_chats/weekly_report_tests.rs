// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::mission::schedule;
use chrono::{DateTime, Duration, Utc};
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";

#[test]
fn weekly_report_does_not_invent_an_owner_from_a_project_or_thread() -> anyhow::Result<()> {
    let root = fixture()?;
    let conn = open_store(root.path())?;
    assert_eq!(
        conn.execute("DELETE FROM business_users WHERE user_id='owner'", [])?,
        1
    );
    drop(conn);
    crate::business_os::reconcile_project_reports(root.path())?;
    assert!(schedule::list_tasks(root.path())?.is_empty());
    assert_eq!(count(root.path(), THREADS)?, 1);
    assert_eq!(count(root.path(), "user_thread_messages")?, 0);
    Ok(())
}

pub(super) fn fixture() -> anyhow::Result<TempDir> {
    let root = supervisor_turns::fixture()?;
    // Automatic work revalidates a persisted active user. The shared control
    // fixture has only project/thread records and a trusted local actor.
    let _ = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner",
        "Project owner",
        "admin",
        store::now_ms() as i64,
    )?;
    patch_project(root.path(), |v| {
        v["jour_fixe"] = json!({"weekday":1,"time":"13:00","timezone":"Europe/Berlin"})
    })?;
    Ok(root)
}
fn patch_project(root: &Path, patch: impl FnOnce(&mut Value)) -> anyhow::Result<()> {
    let conn = open_store(root)?;
    let mut value = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    patch(&mut value);
    store::upsert_business_record(&conn, "workjet_projects", "project", 2, value)?;
    Ok(())
}
fn task(root: &Path) -> anyhow::Result<schedule::ScheduledTaskView> {
    crate::business_os::reconcile_project_reports(root)?;
    let tasks: Vec<_> = schedule::list_tasks(root)?
        .into_iter()
        .filter(|task| task.name.starts_with("workjet-weekly-report:"))
        .collect();
    assert_eq!(tasks.len(), 1);
    Ok(tasks.into_iter().next().unwrap())
}
fn instant(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[test]
fn weekly_project_report_uses_one_schedule_and_preserves_due_and_manual_pause() -> anyhow::Result<()>
{
    let root = fixture()?;
    let first = task(root.path())?;
    assert_eq!(first.cron_expr, "0 13 * * 1");
    assert_eq!(first.calendar.timezone, "Europe/Berlin");
    assert_eq!(first.thread_key, format!("business-os/threads/{THREAD}"));
    let repeated = task(root.path())?;
    assert_eq!(first.task_id, repeated.task_id);
    assert_eq!(first.next_run_at, repeated.next_run_at);
    assert_eq!(first.updated_at, repeated.updated_at);
    schedule::set_task_enabled(root.path(), &first.task_id, false)?;
    assert!(!task(root.path())?.enabled);
    patch_project(root.path(), |v| v["jour_fixe"]["time"] = json!("14:00"))?;
    let updated = task(root.path())?;
    assert!(!updated.enabled);
    assert_eq!(updated.cron_expr, "0 14 * * 1");
    assert_eq!(updated.task_id, first.task_id);
    Ok(())
}

#[test]
fn weekly_report_test_time_admits_the_real_supervisor_turn_with_owner_receipt() -> anyhow::Result<()>
{
    let root = fixture()?;
    let first = task(root.path())?;
    let due = instant(first.next_run_at.as_deref().unwrap());
    assert_eq!(
        schedule::emit_due_task_at(root.path(), &first.task_id, due - Duration::seconds(1))?
            .emitted_count,
        0
    );
    let emitted = schedule::emit_due_task_at(root.path(), &first.task_id, due)?;
    assert_eq!(emitted.emitted_count, 1);
    assert_eq!(count(root.path(), THREADS)?, 1);
    assert_eq!(count(root.path(), "user_thread_messages")?, 1);
    let run = &emitted.emitted_runs[0];
    let operation = stable_id(
        "workjet_weekly_report",
        &[&first.task_id, &run.scheduled_for],
    );
    let receipt = crate::mission::channels::business_command_projection(root.path(), &operation)?;
    let command_id = receipt["result"]["turn"]["command_id"]
        .as_str()
        .context("report command")?;
    let queued =
        crate::mission::channels::load_queue_task_for_business_os_command(root.path(), command_id)?
            .context("report queue")?;
    assert_eq!(queued.message_key, run.message_key);
    let context =
        crate::mission::channels::inspect_business_command(root.path(), command_id)?.unwrap();
    assert_eq!(context["command"]["payload"]["thread_id"], THREAD);
    assert_eq!(
        context["command"]["payload"]["thread_key"],
        first.thread_key
    );
    assert_eq!(context["command"]["payload"]["risk_class"], "internal");
    let native = store::load_business_command(&open_store(root.path())?, command_id)?;
    assert_eq!(native.client_context["actor"]["id"], "owner");
    assert_eq!(native.record_id.as_deref(), Some("project"));
    assert!(queued.prompt.contains("merged PRs"));
    assert!(
        task(root.path())?
            .next_run_at
            .as_deref()
            .map(instant)
            .unwrap()
            > due
    );
    assert_eq!(
        schedule::emit_due_task_at(root.path(), &first.task_id, due)?.emitted_count,
        0
    );
    Ok(())
}

#[test]
fn weekly_report_replaying_a_crash_before_schedule_receipt_does_not_duplicate_a_turn(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let first = task(root.path())?;
    let time = first.next_run_at.as_deref().unwrap();
    let emitted = crate::business_os::emit_project_report(root.path(), &first, time)?;
    let replayed = crate::business_os::emit_project_report(root.path(), &first, time)?;
    assert_eq!(emitted, replayed);
    assert_eq!(count(root.path(), "user_thread_messages")?, 1);
    Ok(())
}

#[test]
fn weekly_report_archive_clear_deleted_or_foreign_project_stops_automatic_emission(
) -> anyhow::Result<()> {
    for kind in ["archive", "clear", "deleted", "foreign"] {
        let root = fixture()?;
        let first = task(root.path())?;
        patch_project(root.path(), |v| match kind {
            "archive" => v["status"] = json!("archived"),
            "clear" => {
                v.as_object_mut().unwrap().remove("jour_fixe");
            }
            "deleted" => v["is_deleted"] = json!(true),
            _ => v["owner_user_id"] = json!("foreign"),
        })?;
        assert!(!task(root.path())?.enabled, "{kind}");
        assert!(
            crate::business_os::emit_project_report(
                root.path(),
                &first,
                first.next_run_at.as_deref().unwrap()
            )
            .is_err(),
            "{kind}"
        );
        assert_eq!(count(root.path(), "user_thread_messages")?, 0);
    }
    Ok(())
}

#[test]
fn weekly_report_revoked_user_or_conflicting_history_cannot_use_an_old_schedule(
) -> anyhow::Result<()> {
    for inactive in [true, false] {
        let root = fixture()?;
        let first = task(root.path())?;
        let conn = open_store(root.path())?;
        if inactive {
            assert_eq!(
                conn.execute(
                    "UPDATE business_users SET active=0 WHERE user_id='owner'",
                    []
                )?,
                1
            );
        } else {
            let mut thread = outbound_load_record(&conn, THREADS, THREAD)?.unwrap();
            thread["source_record_id"] = json!("another-project");
            store::upsert_business_record(&conn, THREADS, THREAD, 3, thread)?;
        }
        drop(conn);
        assert!(!task(root.path())?.enabled);
        assert!(crate::business_os::emit_project_report(
            root.path(),
            &first,
            first.next_run_at.as_deref().unwrap()
        )
        .is_err());
        assert_eq!(count(root.path(), "user_thread_messages")?, 0);
    }
    Ok(())
}

#[test]
fn weekly_report_changed_configuration_rejects_stale_route_and_reschedules_once(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let first = task(root.path())?;
    patch_project(root.path(), |v| {
        v["jour_fixe"] = json!({"weekday":5,"time":"14:30","timezone":"America/New_York"})
    })?;
    assert!(crate::business_os::emit_project_report(
        root.path(),
        &first,
        first.next_run_at.as_deref().unwrap()
    )
    .is_err());
    let changed = task(root.path())?;
    assert_eq!(changed.task_id, first.task_id);
    assert_eq!(changed.cron_expr, "30 14 * * 5");
    assert_eq!(changed.calendar.timezone, "America/New_York");
    assert_eq!(count(root.path(), "user_thread_messages")?, 0);
    Ok(())
}

#[test]
fn weekly_report_forged_schedule_does_not_create_a_message_or_queue_turn() -> anyhow::Result<()> {
    for kind in ["thread", "owner", "unknown", "cron", "zone"] {
        let root = fixture()?;
        let mut first = task(root.path())?;
        match kind {
            "thread" => first.thread_key = "business-os/threads/foreign".into(),
            "cron" => first.cron_expr = "* * * * *".into(),
            "zone" => first.calendar.timezone = "UTC".into(),
            _ => {
                let (marker, raw) = first.prompt.split_once(':').unwrap();
                let mut route: Value = serde_json::from_str(raw)?;
                if kind == "owner" {
                    route["owner_user_id"] = json!("foreign");
                } else {
                    route["untrusted"] = json!(true);
                }
                first.prompt = format!("{marker}:{}", route);
            }
        }
        assert!(
            crate::business_os::emit_project_report(
                root.path(),
                &first,
                first.next_run_at.as_deref().unwrap()
            )
            .is_err(),
            "{kind}"
        );
        assert_eq!(count(root.path(), "user_thread_messages")?, 0);
    }
    Ok(())
}

#[test]
fn weekly_report_storage_failure_keeps_the_existing_schedule_enabled() -> anyhow::Result<()> {
    let root = fixture()?;
    let first = task(root.path())?;
    let before = serde_json::to_value(schedule::list_tasks(root.path())?)?;
    open_store(root.path())?.execute("DROP TABLE business_users", [])?;
    assert!(crate::business_os::reconcile_project_reports(root.path()).is_err());
    let retained = schedule::list_tasks(root.path())?;
    // Preparation and the weekly report now coexist. Unknown authority must
    // preserve both complete schedules, including their due times and pauses.
    assert_eq!(serde_json::to_value(&retained)?, before);
    let report = retained.iter().find(|task| task.task_id == first.task_id).unwrap();
    assert!(report.enabled);
    assert_eq!(report.next_run_at, first.next_run_at);
    assert_eq!(count(root.path(), "user_thread_messages")?, 0);
    Ok(())
}
