use super::*;

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("ctox-schedule-calendar-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create isolated root");
        Self(root)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn berlin(lead_minutes: u16) -> ScheduleCalendar {
    ScheduleCalendar {
        timezone: "Europe/Berlin".into(),
        lead_minutes,
    }
}

fn request() -> ScheduleEnsureRequest {
    ScheduleEnsureRequest {
        name: "prepare existing supervisor meeting".into(),
        cron_expr: "0 14 * * 4".into(),
        prompt: "Prepare the next meeting against its confirmed goal revision.".into(),
        thread_key: "fixture/existing-supervisor".into(),
        skill: Some("workjet-jour-fixe".into()),
    }
}

#[test]
fn weekly_preparation_tracks_iana_zone_in_winter_and_summer() -> Result<()> {
    for (after, expected) in [
        ("2026-01-07T00:00:00Z", "2026-01-08T11:00:00+00:00"),
        ("2026-07-01T00:00:00Z", "2026-07-02T10:00:00+00:00"),
    ] {
        assert_eq!(
            next_run_after("0 14 * * 4", &berlin(120), parse_rfc3339_utc(after)?)?.as_deref(),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
fn preparation_before_midnight_preserves_the_meetings_weekday() -> Result<()> {
    assert_eq!(
        next_run_after(
            "30 0 * * 1",
            &berlin(120),
            parse_rfc3339_utc("2026-07-04T00:00:00Z")?
        )?
        .as_deref(),
        Some("2026-07-05T20:30:00+00:00")
    );
    Ok(())
}

#[test]
fn dst_gap_is_skipped_and_fold_produces_only_one_occurrence() -> Result<()> {
    assert_eq!(
        next_run_after(
            "30 2 * * 0",
            &berlin(120),
            parse_rfc3339_utc("2026-03-28T00:00:00Z")?
        )?
        .as_deref(),
        Some("2026-04-04T22:30:00+00:00")
    );
    let first = next_run_after(
        "30 2 * * 0",
        &berlin(120),
        parse_rfc3339_utc("2026-10-24T00:00:00Z")?,
    )?
    .unwrap();
    assert_eq!(first, "2026-10-24T22:30:00+00:00");
    assert_eq!(
        next_run_after("30 2 * * 0", &berlin(120), parse_rfc3339_utc(&first)?)?.as_deref(),
        Some("2026-10-31T23:30:00+00:00")
    );
    // 10:00 on transition Sunday is still prepared exactly two elapsed hours
    // early, rather than applying yesterday's UTC offset to a local cron.
    assert_eq!(
        next_run_after(
            "0 10 * * 0",
            &berlin(120),
            parse_rfc3339_utc("2026-03-28T00:00:00Z")?
        )?
        .as_deref(),
        Some("2026-03-29T06:00:00+00:00")
    );
    Ok(())
}

#[test]
fn invalid_calendar_is_rejected_before_database_creation_or_mutation() -> Result<()> {
    let root = TestRoot::new();
    for calendar in [
        ScheduleCalendar {
            timezone: "not/an-iana-zone".into(),
            lead_minutes: 120,
        },
        berlin(1441),
    ] {
        assert!(ensure_task_with_calendar(&root.0, request(), calendar).is_err());
        assert!(!resolve_db_path(&root.0).exists());
    }
    let old = ensure_task_with_calendar(&root.0, request(), berlin(120))?;
    assert!(ensure_task_with_calendar(&root.0, request(), berlin(1441)).is_err());
    let retained = list_tasks(&root.0)?;
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].task_id, old.task_id);
    assert_eq!(retained[0].calendar, berlin(120));
    Ok(())
}

#[test]
fn legacy_schedule_rows_migrate_without_changing_utc_or_identity() -> Result<()> {
    let root = TestRoot::new();
    fs::create_dir_all(root.0.join("runtime"))?;
    let conn = Connection::open(resolve_db_path(&root.0))?;
    conn.execute_batch(
        "CREATE TABLE scheduled_tasks (
        task_id TEXT PRIMARY KEY, name TEXT NOT NULL, cron_expr TEXT NOT NULL,
        prompt TEXT NOT NULL, thread_key TEXT NOT NULL, skill TEXT,
        enabled INTEGER NOT NULL, next_run_at TEXT, last_run_at TEXT,
        created_at TEXT NOT NULL, updated_at TEXT NOT NULL
    ); INSERT INTO scheduled_tasks VALUES (
        'existing', 'existing task', '0 14 * * 4', 'original prompt',
        'existing/thread', NULL, 1, '2026-07-02T14:00:00+00:00', NULL,
        '2026-07-01T00:00:00+00:00', '2026-07-01T00:00:00+00:00'
    );",
    )?;
    ensure_schedule_schema(&conn)?;
    ensure_schedule_schema(&conn)?;
    let task = load_task(&conn, "existing")?.unwrap();
    assert_eq!(task.calendar, ScheduleCalendar::default());
    assert_eq!(task.thread_key, "existing/thread");
    assert_eq!(
        task.next_run_at.as_deref(),
        Some("2026-07-02T14:00:00+00:00")
    );
    assert_eq!(task.prompt, "original prompt");
    assert!(serde_json::to_value(&task)?.get("calendar").is_none());
    assert_eq!(
        next_run_after(
            &task.cron_expr,
            &task.calendar,
            parse_rfc3339_utc("2026-07-01T00:00:00Z")?
        )?
        .as_deref(),
        Some("2026-07-02T14:00:00+00:00")
    );
    Ok(())
}

#[test]
fn unchanged_schedule_ensure_preserves_due_deadline_and_run_history() -> Result<()> {
    let root = TestRoot::new();
    let first = ensure_task_with_calendar(&root.0, request(), berlin(120))?;
    let conn = open_schedule_db(&root.0)?;
    let due_at = now_utc() - Duration::minutes(1);
    let due_text = due_at.to_rfc3339();
    let last_run = (due_at - Duration::days(7)).to_rfc3339();
    conn.execute(
        "UPDATE scheduled_tasks SET next_run_at=?2, last_run_at=?3, updated_at=?3 WHERE task_id=?1",
        params![first.task_id, due_text, last_run],
    )?;
    let due = load_task(&conn, &first.task_id)?.unwrap();
    let mut equal = request();
    equal.cron_expr = format!(" {} ", equal.cron_expr);
    equal.prompt = format!(" {} ", equal.prompt);
    let kept = ensure_task_with_calendar(&root.0, equal, berlin(120))?;
    assert_eq!(kept.task_id, due.task_id);
    assert_eq!(kept.next_run_at, due.next_run_at);
    assert_eq!(kept.last_run_at, due.last_run_at);
    assert_eq!(kept.updated_at, due.updated_at);
    assert_eq!(kept.created_at, due.created_at);
    assert_eq!(list_due_tasks(&conn, &now_utc())?.len(), 1);

    // A real appointment change still calculates a new deadline without
    // deleting its previous run timestamp or creating another task.
    let changed = ensure_task_with_calendar(&root.0, request(), berlin(0))?;
    assert_eq!(changed.task_id, due.task_id);
    assert_eq!(changed.calendar, berlin(0));
    assert_ne!(changed.next_run_at, due.next_run_at);
    assert_eq!(changed.last_run_at, due.last_run_at);
    assert_eq!(list_tasks(&root.0)?.len(), 1);
    Ok(())
}

#[test]
fn calendar_survives_upsert_pause_resume_and_real_native_trigger() -> Result<()> {
    let root = TestRoot::new();
    let first = ensure_task_with_calendar(&root.0, request(), berlin(120))?;
    let second = ensure_task_with_calendar(&root.0, request(), berlin(120))?;
    assert_eq!(first.task_id, second.task_id);
    assert_eq!(list_tasks(&root.0)?.len(), 1);
    assert!(!set_task_enabled(&root.0, &first.task_id, false)?.enabled);
    let resumed = set_task_enabled(&root.0, &first.task_id, true)?;
    assert_eq!(resumed.calendar, berlin(120));
    let run = emit_task_now(&root.0, &first.task_id)?;
    let conn = open_schedule_db(&root.0)?;
    let (thread, metadata, body): (String, String, String) = conn.query_row(
        "SELECT thread_key, metadata_json, body_text FROM communication_messages WHERE message_key = ?1",
        params![run.message_key],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    )?;
    assert_eq!(thread, "fixture/existing-supervisor");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&metadata)?["skill"],
        "workjet-jour-fixe"
    );
    assert!(body.contains("Calendar occurrence:"));
    assert!(body.contains("Europe/Berlin"));
    assert!(body.contains("preparation lead: 120 minutes"));
    let stored = load_task(&conn, &first.task_id)?.unwrap();
    assert_eq!(stored.calendar, berlin(120));
    let occurrence =
        parse_rfc3339_utc(stored.next_run_at.as_deref().unwrap())? + Duration::minutes(120);
    let local = occurrence.with_timezone(&chrono_tz::Europe::Berlin);
    assert_eq!(local.weekday().num_days_from_monday(), 3);
    assert_eq!((local.hour(), local.minute()), (14, 0));
    // The service's due-task continuation uses the persisted calendar too.
    let (next, enabled) = next_task_state_after_emit(
        false,
        "emitted",
        "0 14 * * 4",
        &berlin(120),
        "2026-07-02T10:00:00+00:00",
        parse_rfc3339_utc("2026-07-02T10:01:00Z")?,
    )?;
    assert!(enabled);
    assert_eq!(next.as_deref(), Some("2026-07-09T10:00:00+00:00"));
    Ok(())
}

#[test]
fn cli_calendar_defaults_are_compatible_and_malformed_values_are_named() -> Result<()> {
    let base: Vec<String> = [
        "add",
        "--name",
        "meeting",
        "--cron",
        "0 14 * * 4",
        "--prompt",
        "prepare",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(
        parse_add_request(&base)?.calendar,
        ScheduleCalendar::default()
    );
    let mut zoned = base.clone();
    zoned.extend(
        ["--timezone", "Europe/Berlin", "--lead-minutes", "120"]
            .into_iter()
            .map(str::to_owned),
    );
    assert_eq!(parse_add_request(&zoned)?.calendar, berlin(120));
    for tail in [
        vec!["--timezone"],
        vec!["--lead-minutes"],
        vec!["--lead-minutes", "--skill", "test"],
        vec!["--lead-minutes", "-1"],
        vec!["--lead-minutes", "1441"],
        vec!["--timezone", "not/a-zone"],
    ] {
        let mut invalid = base.clone();
        invalid.extend(tail.into_iter().map(str::to_owned));
        assert!(parse_add_request(&invalid).is_err());
    }
    Ok(())
}
