// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::mission::{channels, schedule};
use chrono::{DateTime, Duration, Utc};
const PREFIX: &str = "workjet-jour-fixe-prepare:";
fn fixture() -> anyhow::Result<TempDir> {
    super::weekly_reports::fixture()
}
fn task(root: &Path) -> anyhow::Result<schedule::ScheduledTaskView> {
    crate::business_os::reconcile_project_reports(root)?;
    let mut tasks: Vec<_> = schedule::list_tasks(root)?
        .into_iter()
        .filter(|t| t.name.starts_with(PREFIX))
        .collect();
    assert_eq!(tasks.len(), 1);
    Ok(tasks.pop().unwrap())
}
fn instant(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}
fn read(root: &Path, actor: &str, id: &str, payload: Value) -> anyhow::Result<Value> {
    store::accept_rxdb_business_command_with_origin(
        root,
        json!({"id":id,"module":"ctox",
      "command_type":"ctox.workjet.jour_fixe.meeting.read","record_id":"project","payload":payload,
      "client_context":{"actor":{"id":actor,"role":"admin"}}}),
        store::CommandOrigin::TrustedLocal,
    )
}
fn latest(root: &Path) -> anyhow::Result<Value> {
    let value = read(root, "owner", "latest", json!({"project_id":"project"}))?;
    assert_eq!(value["status"], "completed", "{value}");
    Ok(value["result"].clone())
}
fn rejected(value: anyhow::Result<Value>) {
    assert!(
        value.is_err() || value.as_ref().is_ok_and(|v| v["status"] == "failed"),
        "{value:?}"
    );
}
fn patch_project(root: &Path, patch: impl FnOnce(&mut Value)) -> anyhow::Result<()> {
    let conn = open_store(root)?;
    let mut value =
        outbound_load_record(&conn, "workjet_projects", "project")?.context("project")?;
    patch(&mut value);
    store::upsert_business_record(&conn, "workjet_projects", "project", 2, value)?;
    Ok(())
}

#[test]
fn preparation_calendar_is_one_native_task_two_hours_before_the_appointment() -> anyhow::Result<()>
{
    let root = fixture()?;
    let first = task(root.path())?;
    let all = schedule::list_tasks(root.path())?;
    assert_eq!(all.len(), 2);
    let report = all
        .iter()
        .find(|t| t.name.starts_with("workjet-weekly-report:"))
        .context("weekly report")?;
    assert_eq!(first.calendar.lead_minutes, 120);
    assert_eq!(first.calendar.timezone, "Europe/Berlin");
    assert_eq!(first.cron_expr, report.cron_expr);
    assert_eq!(
        instant(first.next_run_at.as_deref().unwrap()) + Duration::hours(2),
        instant(report.next_run_at.as_deref().unwrap())
    );
    let repeated = task(root.path())?;
    assert_eq!(first.task_id, repeated.task_id);
    assert_eq!(first.next_run_at, repeated.next_run_at);
    assert_eq!(first.updated_at, repeated.updated_at);
    Ok(())
}

#[test]
fn test_time_creates_one_planned_meeting_and_the_registered_supervisor_turn() -> anyhow::Result<()>
{
    let root = fixture()?;
    let first = task(root.path())?;
    let due = instant(first.next_run_at.as_deref().unwrap());
    assert_eq!(
        schedule::emit_due_task_at(root.path(), &first.task_id, due - Duration::seconds(1))?
            .emitted_count,
        0
    );
    let result = schedule::emit_due_task_at(root.path(), &first.task_id, due)?;
    assert_eq!(result.emitted_count, 1);
    let meeting = latest(root.path())?;
    assert_eq!(meeting["meeting"]["state"], "planned");
    assert_eq!(meeting["meeting"]["prepare_at_ms"], due.timestamp_millis());
    assert_eq!(
        meeting["meeting"]["scheduled_at_ms"],
        (due + Duration::hours(2)).timestamp_millis()
    );
    assert_eq!(meeting["meeting"]["slides"], json!([]));
    let key = result.emitted_runs[0].message_key.clone();
    assert_eq!(meeting["preparation_task_id"], key);
    let queue = channels::load_queue_task(root.path(), &key)?.context("preparation task")?;
    assert_eq!(
        queue.thread_key,
        meeting["meeting"]["supervisor"]["ctox_thread_key"]
            .as_str()
            .unwrap()
    );
    assert!(queue.prompt.contains("JourFix"));
    assert!(queue
        .prompt
        .contains("Only the owner's explicit confirmation"));
    assert!(queue
        .prompt
        .contains("Never invent numeric metrics or completed work."));
    assert!(
        crate::skill_store::load_skill_deliverable_contract(root.path(), "jour-fix")?.is_some()
    );
    assert!(queue
        .prompt
        .contains(meeting["meeting"]["id"].as_str().unwrap()));
    assert_eq!(count(root.path(), "user_thread_messages")?, 1);
    assert_eq!(
        schedule::emit_due_task_at(root.path(), &first.task_id, due)?.emitted_count,
        0
    );
    Ok(())
}

#[test]
fn replay_after_admission_before_schedule_receipt_retains_metadata_and_one_turn(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let first = task(root.path())?;
    let due = first.next_run_at.as_deref().unwrap();
    let emitted = crate::business_os::emit_project_report(root.path(), &first, due)?;
    let before = latest(root.path())?;
    let conn = open_store(root.path())?;
    let mut saved = before["meeting"].clone();
    saved["state"] = json!("failed");
    saved["revision"] = json!(3);
    saved["error"] = json!("speech_missing_credential");
    conn.execute(
        "UPDATE workjet_jour_fixe_meetings SET metadata_json=?2 WHERE meeting_id=?1",
        rusqlite::params![
            saved["id"].as_str().unwrap(),
            serde_json::to_string(&saved)?
        ],
    )?;
    drop(conn);
    assert_eq!(
        emitted,
        crate::business_os::emit_project_report(root.path(), &first, due)?
    );
    let after = read(
        root.path(),
        "owner",
        "after-replay",
        json!({"project_id":"project"}),
    )?;
    assert_eq!(after["status"], "completed");
    assert_eq!(after["result"]["meeting"], saved);
    assert_eq!(count(root.path(), "user_thread_messages")?, 1);
    Ok(())
}

#[test]
fn latest_meeting_read_is_empty_without_creating_a_meeting_table() -> anyhow::Result<()> {
    let root = fixture()?;
    assert_eq!(latest(root.path())?["meeting"], Value::Null);
    rejected(read(
        root.path(),
        "owner",
        "explicit-missing-meeting",
        json!({"project_id":"project","meeting_id":"missing-meeting"}),
    ));
    let exists: bool = open_store(root.path())?.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_jour_fixe_meetings')", [], |r| r.get(0))?;
    assert!(!exists);
    Ok(())
}

#[test]
fn foreign_owner_and_forged_payload_cannot_read_private_meeting_metadata() -> anyhow::Result<()> {
    let root = fixture()?;
    let first = task(root.path())?;
    crate::business_os::emit_project_report(
        root.path(),
        &first,
        first.next_run_at.as_deref().unwrap(),
    )?;
    rejected(read(
        root.path(),
        "foreign",
        "foreign",
        json!({"project_id":"project"}),
    ));
    rejected(read(
        root.path(),
        "owner",
        "forged-owner",
        json!({"project_id":"project","owner_user_id":"owner"}),
    ));
    rejected(read(
        root.path(),
        "owner",
        "unknown-meeting",
        json!({"project_id":"project","meeting_id":"foreign-meeting"}),
    ));
    rejected(read(
        root.path(),
        "owner",
        "spaced-project",
        json!({"project_id":" project "}),
    ));
    let own = latest(root.path())?;
    let replay = read(
        root.path(),
        "owner",
        "same-meeting",
        json!({"project_id":"project","meeting_id":own["meeting"]["id"]}),
    )?;
    assert_eq!(replay["result"]["meeting"], own["meeting"]);
    let projected = channels::business_command_projection(root.path(), "same-meeting")?;
    assert_eq!(
        privacy::document_visible_to_actor(root.path(), "business_commands", &projected, "foreign"),
        Some(false)
    );
    assert_eq!(
        privacy::document_visible_to_actor(root.path(), "business_commands", &projected, "owner"),
        Some(true)
    );
    Ok(())
}

#[test]
fn archive_delete_clear_or_foreign_ownership_stops_old_preparation_without_a_turn(
) -> anyhow::Result<()> {
    for reason in ["archived", "deleted", "clear", "foreign"] {
        let root = fixture()?;
        let first = task(root.path())?;
        patch_project(root.path(), |v| match reason {
            "archived" => v["status"] = json!("archived"),
            "deleted" => v["is_deleted"] = json!(true),
            "clear" => v["jour_fixe"] = Value::Null,
            _ => v["owner_user_id"] = json!("foreign"),
        })?;
        assert!(!task(root.path())?.enabled, "{reason}");
        assert!(
            crate::business_os::emit_project_report(
                root.path(),
                &first,
                first.next_run_at.as_deref().unwrap()
            )
            .is_err(),
            "{reason}"
        );
        assert_eq!(count(root.path(), "user_thread_messages")?, 0);
    }
    Ok(())
}

#[test]
fn revoked_native_owner_or_changed_supervisor_history_cannot_prepare_or_read() -> anyhow::Result<()>
{
    for revoked in [true, false] {
        let root = fixture()?;
        let first = task(root.path())?;
        crate::business_os::emit_project_report(
            root.path(),
            &first,
            first.next_run_at.as_deref().unwrap(),
        )?;
        let conn = open_store(root.path())?;
        if revoked {
            conn.execute(
                "UPDATE business_users SET active=0 WHERE user_id='owner'",
                [],
            )?;
        } else {
            let mut thread =
                outbound_load_record(&conn, THREADS, "cc6cfe73-2824-4360-9daf-3b3efb079931")?
                    .unwrap();
            thread["source_record_id"] = json!("foreign");
            store::upsert_business_record(
                &conn,
                THREADS,
                "cc6cfe73-2824-4360-9daf-3b3efb079931",
                3,
                thread,
            )?;
        }
        drop(conn);
        assert!(!task(root.path())?.enabled);
        rejected(read(
            root.path(),
            "owner",
            "revoked-read",
            json!({"project_id":"project"}),
        ));
        assert!(crate::business_os::emit_project_report(
            root.path(),
            &first,
            first.next_run_at.as_deref().unwrap()
        )
        .is_err());
        assert_eq!(count(root.path(), "user_thread_messages")?, 1);
    }
    Ok(())
}

#[test]
fn changing_calendar_rejects_old_route_and_preserves_operator_pause() -> anyhow::Result<()> {
    let root = fixture()?;
    let first = task(root.path())?;
    schedule::set_task_enabled(root.path(), &first.task_id, false)?;
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
    assert!(!changed.enabled);
    assert_eq!(changed.calendar.lead_minutes, 120);
    assert_eq!(changed.calendar.timezone, "America/New_York");
    assert_eq!(changed.cron_expr, "30 14 * * 5");
    assert_eq!(count(root.path(), "user_thread_messages")?, 0);
    Ok(())
}
