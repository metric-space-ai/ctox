#[test]
fn crew_maintenance_failure_and_missing_outbox_preserve_status_and_events() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("ALTER TABLE communication_routing_state ADD COLUMN leased_at TEXT;")?;
    crate::crew::ensure_schema(&conn)?;
    conn.execute("INSERT INTO communication_routing_state(message_key,route_status,updated_at) VALUES('task','leased',?1)", [Utc::now().to_rfc3339()])?;
    conn.execute("INSERT INTO ctox_harness_flow_events VALUES('visible','worker.phase','Working','','task',NULL,NULL,'{}',?1)", [Utc::now().to_rfc3339()])?;
    conn.execute_batch(
        "DROP TABLE crew_projection_tombstones;
        DROP INDEX idx_crew_attempt_unstarted;
        ALTER TABLE crew_attempts DROP COLUMN started_at;",
    )?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    refresh_selected(
        root.path(),
        &WorkerSnapshot::default(),
        &mut writer,
        MAINTENANCE | STATUS | EVENTS,
    )?;
    assert_eq!(
        record(root.path(), "ctox_harness_events", "visible")?["id"],
        "visible"
    );
    assert!(record(root.path(), "ctox_harness_status", "harness")?["updated_at_ms"].is_number());
    crate::crew::ensure_schema(&conn)?;
    conn.execute("INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at) VALUES('orphan','closed','crew-milo','2020-01-01T00:00:00Z')", [])?;
    conn.execute_batch("CREATE TRIGGER fixture_retention_failure BEFORE DELETE ON crew_attempts BEGIN SELECT RAISE(FAIL,'maintenance fixture'); END;")?;
    for busy in [true, false] {
        refresh_selected(
            root.path(),
            &WorkerSnapshot {
                busy,
                ..Default::default()
            },
            &mut writer,
            MAINTENANCE | STATUS | EVENTS,
        )?;
        assert!(writer.crew_maintenance_warned);
        assert_eq!(
            record(root.path(), "ctox_harness_status", "harness")?["busy"],
            busy
        );
    }
    conn.execute_batch("DROP TRIGGER fixture_retention_failure;")?;
    refresh_selected(
        root.path(),
        &WorkerSnapshot::default(),
        &mut writer,
        MAINTENANCE | STATUS,
    )?;
    assert!(!writer.crew_maintenance_warned);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM crew_attempts WHERE attempt_id='orphan'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );
    Ok(())
}

#[test]
fn crew_learning_retention_caches_post_delete_stamp_and_uses_member_index() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("ALTER TABLE communication_routing_state ADD COLUMN leased_at TEXT;")?;
    crate::crew::ensure_schema(&conn)?;
    for n in 0..201 {
        conn.execute("INSERT INTO crew_member_learnings(id,member_id,text,normalized_text,kind,scope_json,evidence_run_id,created_at) VALUES(?1,'crew-milo',?1,?1,'pitfall','{}','run',?2)", params![format!("learning-{n:03}"), Utc::now().to_rfc3339()])?;
    }
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_crew(root.path(), &conn, &mut writer)?;
    let updated: String = conn.query_row(
        "SELECT updated_at FROM crew_members WHERE id='crew-milo'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(writer.crew_sources["crew-milo"].0, updated);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM crew_member_learnings", [], |r| r
            .get::<_, i64>(0))?,
        200
    );
    let plan = writer.inner.source_connection().prepare("EXPLAIN QUERY PLAN SELECT record_id FROM business_records WHERE collection='ctox_crew_learnings' AND deleted=0 AND json_extract(payload_json,'$.member_id')=?2 AND record_id>?1 ORDER BY record_id LIMIT 128")?
        .query_map(params!["", "crew-milo"], |r|r.get::<_,String>(3))?.collect::<rusqlite::Result<Vec<_>>>()?.join("\n");
    assert!(
        plan.contains("idx_crew_projection_learning_member_id"),
        "{plan}"
    );
    assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    conn.execute_batch("DROP TABLE crew_member_learnings;")?;
    project_crew(root.path(), &conn, &mut writer)?;
    Ok(())
}

#[test]
fn crew_status_recovers_durable_diagnostic_without_process_memory() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute("INSERT INTO ctox_harness_flow_events VALUES('warning','crew_selection_unavailable','no active crew member available','',NULL,NULL,NULL,'{}','2026-01-01T00:00:00Z')", [])?;
    // Repaired historical selections are not a current recovery signal.
    conn.execute("INSERT INTO ctox_harness_flow_events VALUES('repaired','crew_selected','selected: old work','',NULL,NULL,NULL,'{\"repaired\":true}','2026-01-02T00:00:00Z')", [])?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_status(root.path(), &conn, &mut writer, &WorkerSnapshot::default())?;
    assert!(
        record(root.path(), "ctox_harness_status", "harness")?["last_error"]
            .as_str()
            .unwrap()
            .contains("no active crew member")
    );
    conn.execute("INSERT INTO ctox_harness_flow_events VALUES('success','crew_selected','selected: recovered','',NULL,NULL,NULL,'{}','2026-01-01T00:00:01Z')", [])?;
    let snapshot = persisted_snapshot(root.path())?;
    project_status(root.path(), &conn, &mut writer, &snapshot)?;
    assert!(record(root.path(), "ctox_harness_status", "harness")?["last_error"].is_null());
    Ok(())
}

#[test]
fn crew_attempt_retention_runs_only_on_periodic_maintenance() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("ALTER TABLE communication_routing_state ADD COLUMN leased_at TEXT;")?;
    crate::crew::ensure_schema(&conn)?;
    conn.execute(
        "INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at)
        VALUES('orphan','closed','crew-milo','2020-01-01T00:00:00Z')",
        [],
    )?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    refresh_selected(root.path(), &WorkerSnapshot::default(), &mut writer, QUEUE)?;
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM crew_attempts", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    refresh_selected(
        root.path(),
        &WorkerSnapshot::default(),
        &mut writer,
        QUEUE | MAINTENANCE,
    )?;
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM crew_attempts", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn crew_events_only_wake_does_not_read_crew_tables() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("CREATE TABLE crew_members(id TEXT);")?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    refresh_selected(root.path(), &WorkerSnapshot::default(), &mut writer, EVENTS)?;
    Ok(())
}

#[test]
fn normal_event_wake_skips_selection_repair_scan() -> Result<()> {
    let (root, conn) = setup()?;
    // Deliberately incomplete attempts/outbox: probing existence is harmless,
    // but a repair SELECT would fail. Normal events must not read this history.
    conn.execute_batch("CREATE TABLE crew_attempts(attempt_id TEXT);
        CREATE TABLE crew_projection_tombstones(id TEXT);
        INSERT INTO communication_routing_state VALUES('task','leased','2026-01-01');
        INSERT INTO ctox_harness_flow_events VALUES('phase','worker.phase','Working','','task',NULL,NULL,'{}','2026-01-01T00:00:00Z');")?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    refresh_selected(root.path(), &WorkerSnapshot::default(), &mut writer, EVENTS)?;
    assert_eq!(
        record(root.path(), "ctox_harness_events", "phase")?["title"],
        "Working"
    );
    assert!(crate::crew::repair_selection_events(root.path(), &conn).is_err());
    Ok(())
}

#[test]
fn crew_unchanged_source_skips_learning_reads_and_resting_expires() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("ALTER TABLE communication_routing_state ADD COLUMN leased_at TEXT;")?;
    crate::crew::ensure_schema(&conn)?;
    let now = Utc::now();
    conn.execute("INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at,started_at,finalized_at,succeeded)
        VALUES('recent','closed','crew-milo',?1,?1,?1,0)",[now.to_rfc3339()])?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_crew(root.path(), &conn, &mut writer)?;
    assert_eq!(
        record(root.path(), "ctox_crew_members", "crew-milo")?["state"],
        "resting_after_failure"
    );
    // No member timestamp change: the derived state itself invalidates its cache.
    conn.execute(
        "UPDATE crew_attempts SET finalized_at=?1 WHERE attempt_id='recent'",
        [(now - chrono::Duration::hours(25)).to_rfc3339()],
    )?;
    project_crew(root.path(), &conn, &mut writer)?;
    assert_eq!(
        record(root.path(), "ctox_crew_members", "crew-milo")?["state"],
        "home"
    );
    assert_eq!(writer.crew_sources.len(), 4);
    conn.execute_batch("DROP TABLE crew_member_learnings;")?;
    project_crew(root.path(), &conn, &mut writer)?;
    Ok(())
}

#[test]
fn crew_selection_warning_reaches_events_and_status() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("ALTER TABLE communication_routing_state ADD COLUMN leased_at TEXT;")?;
    crate::crew::ensure_schema(&conn)?;
    conn.execute("INSERT INTO communication_routing_state(message_key,route_status,updated_at) VALUES('task','leased',?1)",[Utc::now().to_rfc3339()])?;
    // Incomplete lease fixture causes optional crew preparation to fail safely.
    assert!(crate::crew::prepare_attempt_or_continue(
        root.path(),
        &["task".into()],
        "worker",
        "attempt",
        None,
        &json!({}),
        None,
        "Inspect",
        None,
    )
    .is_none());
    // Fixture ledger has a reduced shape; insert the corresponding durable warning
    // explicitly to test the independent projection and its kind allowlist.
    conn.execute("INSERT INTO ctox_harness_flow_events VALUES('warning','crew_selection_unavailable','Crew unavailable','','task',NULL,NULL,'{}',?1)",[Utc::now().to_rfc3339()])?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_events(root.path(), &conn, &mut writer)?;
    project_status(root.path(), &conn, &mut writer, &WorkerSnapshot::default())?;
    assert_eq!(
        record(root.path(), "ctox_harness_events", "warning")?["kind"],
        "crew_selection_unavailable"
    );
    assert!(
        record(root.path(), "ctox_harness_status", "harness")?["last_error"]
            .as_str()
            .unwrap()
            .contains("Crew selection unavailable")
    );
    Ok(())
}

use super::*;
use tempfile::TempDir;

#[test]
fn correlated_run_lookups_seek_attempt_index_instead_of_scanning_history() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("INSERT INTO worker_attempt_finalizations VALUES('a','work','failed','failed','2020-01-01T00:00:00Z','2020-01-01T00:01:00Z','2020-01-01T00:01:00Z','timeout',0);
        WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10000)
        INSERT INTO ctox_harness_flow_events SELECT 'event-'||x,'worker.phase','','','task','work',1,json_object('attempt_id','unrelated-'||x),'2020-01-01T00:00:00Z' FROM n;
        INSERT INTO ctox_harness_flow_events VALUES('match','worker.phase','','','task','work',1,'{\"attempt_id\":\"a\"}','2020-01-01T00:00:00Z');")?;
    let indexed = core(root.path())?;
    // Inspect the production query, including its third correlated EXISTS.
    let sql = finalized_runs_sql(false);
    let plan = indexed
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
        .query_map(params!["", 128], |r| r.get::<_, String>(3))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .join("\n");
    assert_eq!(
        plan.matches("SEARCH e USING INDEX idx_cockpit_flow_attempt")
            .count(),
        3,
        "{plan}"
    );
    assert!(!plan.contains("SCAN e"), "{plan}");
    let mut production = indexed.prepare(&sql)?;
    assert_eq!(
        production.query_row(params!["", 128], |r| r.get::<_, String>(0))?,
        "a"
    );
    assert!(production.get_status(rusqlite::StatementStatus::VmStep) < 1000);
    for projection in ["e.work_id", "e.message_key"] {
        let sql = format!(
            "SELECT (SELECT {projection} FROM ctox_harness_flow_events e
            WHERE json_extract(e.metadata_json,'$.attempt_id')=+f.attempt_id
            AND {projection} IS NOT NULL ORDER BY e.created_at LIMIT 1)
            FROM worker_attempt_finalizations f"
        );
        let plan = indexed
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
            .query_map([], |r| r.get::<_, String>(3))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .join("\n");
        assert!(
            plan.contains("SEARCH e USING INDEX idx_cockpit_flow_attempt"),
            "{plan}"
        );
        assert!(!plan.contains("SCAN e"), "{plan}");
        let mut statement = indexed.prepare(&sql)?;
        let value: String = statement.query_row([], |r| r.get(0))?;
        assert_eq!(
            value,
            if projection == "e.work_id" {
                "work"
            } else {
                "task"
            }
        );
        assert!(statement.get_status(rusqlite::StatementStatus::VmStep) < 100);
    }
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_runs(root.path(), &indexed, &mut writer)?;
    let run = record(root.path(), "ctox_runs", "a")?;
    assert_eq!(run["task_id"], "task");
    assert_eq!(run["work_id"], "work");
    assert_eq!(run["error_text"], "timeout");
    Ok(())
}

#[test]
#[ignore = "representative 278716-event pump profile; run explicitly with --nocapture"]
fn profile_all_pump_phases_at_tenant_scale() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("ALTER TABLE communication_routing_state ADD COLUMN leased_at TEXT;")?;
    crate::crew::ensure_schema(&conn)?;
    conn.execute_batch("BEGIN;")?;
    for n in 0..600 {
        let task = format!("task-{n:04}");
        let attempt = format!("attempt-{n:04}");
        let status = if n < 3 {
            "leased"
        } else if n < 22 {
            "pending"
        } else {
            "failed"
        };
        conn.execute("INSERT INTO communication_routing_state(message_key,route_status,updated_at) VALUES(?1,?2,'2020-01-01T00:00:00Z')", params![task, status])?;
        conn.execute("INSERT INTO worker_attempt_finalizations VALUES(?1,?2,'failed','failed','2020-01-01T00:00:00Z','2020-01-01T00:01:00Z','2020-01-01T00:01:00Z','handshake timeout',0)", params![attempt,task])?;
        conn.execute("INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at,started_at,finalized_at,succeeded) VALUES(?1,?2,'crew-milo','2020-01-01T00:00:00Z','2020-01-01T00:00:00Z','2020-01-01T00:01:00Z',0)",params![attempt,task])?;
    }
    {
        let mut insert = conn.prepare("INSERT INTO ctox_harness_flow_events VALUES(?1,'worker.tool_started','tool','','task-'||printf('%04d',?2),'work',1,?3,'2020-01-01T00:00:00Z')")?;
        for n in 0..278_716 {
            insert.execute(params![format!("event-{n:06}"), n % 600,
                json!({"attempt_id":format!("attempt-{:04}",n%600),"turn_id":format!("turn-{}",n%600),"tool":{"name":"rg","call_id":format!("call-{n}")}}).to_string()])?;
        }
    }
    conn.execute_batch("COMMIT;")?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    for (pass, flags) in [
        ("cold", ALL | MAINTENANCE),
        ("warm", ALL),
        ("unchanged", ALL),
    ] {
        let mut timing = ProjectionTiming::new(root.path(), flags);
        refresh_measured(
            root.path(),
            &WorkerSnapshot::default(),
            &mut writer,
            flags,
            &mut timing,
        )?;
        for (name, elapsed) in &timing.phases {
            eprintln!(
                "PROFILE pass={pass} phase={name} us={}",
                elapsed.as_micros()
            );
        }
    }
    assert!(
        conn.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events", [], |r| r
            .get::<_, i64>(
            0
        ))? >= 278_716
    );
    Ok(())
}

fn setup() -> Result<(TempDir, Connection)> {
    let root = tempfile::tempdir()?;
    std::fs::create_dir_all(root.path().join("runtime"))?;
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    conn.execute_batch("CREATE TABLE communication_routing_state(message_key TEXT PRIMARY KEY,route_status TEXT NOT NULL,updated_at TEXT NOT NULL);
        CREATE TABLE communication_messages(message_key TEXT PRIMARY KEY,channel TEXT,direction TEXT);
        CREATE TRIGGER fixture_queue_identity AFTER INSERT ON communication_routing_state BEGIN INSERT INTO communication_messages VALUES(new.message_key,'queue','inbound'); END;
        CREATE TABLE ctox_harness_flow_events(event_id TEXT PRIMARY KEY,event_kind TEXT,title TEXT,body_text TEXT,message_key TEXT,work_id TEXT,attempt_index INTEGER,metadata_json TEXT,created_at TEXT);
        CREATE TABLE worker_attempt_finalizations(attempt_id TEXT PRIMARY KEY,work_key TEXT,status TEXT,agent_outcome TEXT,created_at TEXT,updated_at TEXT,terminal_at TEXT,error_text TEXT,resumable INTEGER);
        CREATE TABLE api_model_cost_events(provider TEXT,model TEXT,turn_id TEXT,input_tokens INTEGER,cached_input_tokens INTEGER,output_tokens INTEGER,reasoning_output_tokens INTEGER,day TEXT);
        CREATE TABLE api_model_price_rates(provider TEXT,model TEXT,input_usd_per_million REAL,cached_input_usd_per_million REAL,output_usd_per_million REAL,effective_from_day TEXT);")?;
    let rxdb = Connection::open(store::rxdb_store_path(root.path()))?;
    for (name, version) in [
        ("ctox_queue_tasks", 3),
        ("ctox_harness_events", 0),
        ("ctox_harness_status", 0),
        ("ctox_runs", 1),
        ("ctox_crew_members", 0),
        ("ctox_crew_learnings", 0),
    ] {
        rxdb.execute_batch(&format!("CREATE TABLE ctox_business_os__{name}__v{version}(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER DEFAULT 0,lastWriteTime REAL DEFAULT 0,data TEXT NOT NULL);"))?;
    }
    Ok((root, conn))
}

fn record(root: &Path, collection: &str, id: &str) -> Result<Value> {
    let raw: String = store::open_store(root)?.query_row(
        "SELECT payload_json FROM business_records WHERE collection=?1 AND record_id=?2",
        params![collection, id],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}

#[test]
fn malformed_pause_is_visible_without_stopping_admission_and_recovers() -> Result<()> {
    let (root, conn) = setup()?;
    crate::inference::runtime_env::set_runtime_env_value(root.path(), "queue.pause", "{broken")?;
    assert!(!super::super::queue_is_paused(root.path()));
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_status(root.path(), &conn, &mut writer, &WorkerSnapshot::default())?;
    let status = record(root.path(), "ctox_harness_status", "harness")?;
    assert_eq!(status["paused"], false);
    assert!(status["last_error"]
        .as_str()
        .unwrap()
        .contains("Invalid queue.pause"));
    crate::inference::runtime_env::set_runtime_env_value(
        root.path(),
        "queue.pause",
        r#"{"paused":false,"reason":null}"#,
    )?;
    // Also clear a diagnostic restored from the previously persisted singleton.
    let snapshot = persisted_snapshot(root.path())?;
    project_status(root.path(), &conn, &mut writer, &snapshot)?;
    assert!(record(root.path(), "ctox_harness_status", "harness")?["last_error"].is_null());
    Ok(())
}

#[test]
fn event_step_uses_the_plan_at_emission_and_unknown_eligibility_replays() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute(
        "INSERT INTO communication_routing_state VALUES('task','leased','2026-01-01T00:00:00Z')",
        [],
    )?;
    for (id, at, steps) in [
        (
            "plan-1",
            "2026-01-01T00:00:01Z",
            json!([{"step":"one","status":"in_progress"},{"step":"two","status":"pending"}]),
        ),
        (
            "plan-2",
            "2026-01-01T00:00:03Z",
            json!([{"step":"one","status":"completed"},{"step":"two","status":"in_progress"}]),
        ),
    ] {
        conn.execute("INSERT INTO ctox_harness_flow_events VALUES(?1,'worker.plan_updated','Plan','','task',NULL,NULL,?2,?3)",
            params![id, json!({"attempt_id":"attempt","attempt":7,"plan":{"plan":steps}}).to_string(), at])?;
    }
    for (id, at) in [
        ("tool-1", "2026-01-01T00:00:02Z"),
        ("tool-2", "2026-01-01T00:00:04Z"),
    ] {
        conn.execute("INSERT INTO ctox_harness_flow_events VALUES(?1,'worker.tool_started','Tool','','task',NULL,NULL,?2,?3)",
            params![id, json!({"attempt_id":"attempt"}).to_string(), at])?;
    }
    project_events(
        root.path(),
        &conn,
        &mut BusinessProjectionWriter::open(root.path())?,
    )?;
    assert_eq!(
        record(root.path(), "ctox_harness_events", "tool-1")?["step_position"],
        1
    );
    assert_eq!(
        record(root.path(), "ctox_harness_events", "tool-2")?["step_position"],
        2
    );
    assert_eq!(
        record(root.path(), "ctox_harness_events", "tool-1")?["attempt"],
        7
    );
    Ok(())
}

#[test]
fn invalid_required_projection_times_skip_documents_and_replay_after_repair() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute(
        "INSERT INTO communication_routing_state VALUES('task','leased','2026-01-01T00:00:00Z')",
        [],
    )?;
    conn.execute("INSERT INTO ctox_harness_flow_events VALUES('bad-event','worker.phase','Phase','','task',NULL,NULL,'{}','bad')", [])?;
    conn.execute("INSERT INTO worker_attempt_finalizations VALUES('bad-run','work','failed','failed','bad','bad','bad',NULL,0)", [])?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    for _ in 0..2 {
        project_events(root.path(), &conn, &mut writer)?;
        project_runs(root.path(), &conn, &mut writer)?;
        let count: i64 = store::open_store(root.path())?.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection IN ('ctox_harness_events','ctox_runs') AND deleted=0", [], |row| row.get(0))?;
        assert_eq!(
            count, 0,
            "invalid required/index timestamps must not reach replication"
        );
    }
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events", [], |row| {
            row.get::<_, i64>(0)
        })?,
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM worker_attempt_finalizations",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    conn.execute(
        "UPDATE ctox_harness_flow_events SET created_at='2026-01-01T00:00:00Z'",
        [],
    )?;
    conn.execute("UPDATE worker_attempt_finalizations SET terminal_at='1767225603000', updated_at='1767225603000'", [])?;
    let before_replay = Utc::now().timestamp_millis();
    project_events(root.path(), &conn, &mut writer)?;
    project_runs(root.path(), &conn, &mut writer)?;
    let event = record(root.path(), "ctox_harness_events", "bad-event")?;
    assert_eq!(event["created_at_ms"], 1767225600000_i64);
    // The writer stamps observation time, independently of source creation time.
    assert!(event["updated_at_ms"]
        .as_i64()
        .is_some_and(|stamp| (before_replay..=Utc::now().timestamp_millis()).contains(&stamp)));
    let run = record(root.path(), "ctox_runs", "bad-run")?;
    assert_eq!(run["finished_at_ms"], 1767225603000_i64);
    assert!(run["updated_at_ms"]
        .as_i64()
        .is_some_and(|stamp| (before_replay..=Utc::now().timestamp_millis()).contains(&stamp)));
    assert!(run["started_at_ms"].is_null());
    assert!(run["metrics"]["elapsed_ms"].is_null());
    Ok(())
}

#[test]
fn invalid_run_times_remain_unknown_instead_of_epoch_sized_elapsed() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute("INSERT INTO worker_attempt_finalizations VALUES('invalid','work','failed','failed','bad','1767225603000','1767225603000',NULL,0)", [])?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_runs(root.path(), &conn, &mut writer)?;
    let run = record(root.path(), "ctox_runs", "invalid")?;
    assert!(run["started_at_ms"].is_null());
    assert!(run["metrics"]["elapsed_ms"].is_null());
    assert_eq!(run["finished_at_ms"], 1767225603000_i64);
    assert!(
        run.get("attempt_id").is_none(),
        "id alone is the attempt identity"
    );
    assert_eq!(millis("bad"), None);
    assert_eq!(millis("0"), Some(0));
    Ok(())
}

#[test]
fn native_cockpit_queries_use_the_declared_time_task_and_crew_indexes() -> Result<()> {
    let (root, _) = setup()?;
    let _writer = BusinessProjectionWriter::open(root.path())?;
    let conn = store::open_store(root.path())?;
    for (query, index) in [
        ("SELECT record_id FROM business_records WHERE collection='ctox_runs' AND deleted=0 AND json_extract(payload_json,'$.task_id')='task' ORDER BY json_extract(payload_json,'$.finished_at_ms') DESC", "idx_cockpit_run_task_finished"),
        ("SELECT record_id FROM business_records WHERE collection='ctox_runs' AND deleted=0 AND json_extract(payload_json,'$.crew_member_id')='member'", "idx_cockpit_run_crew"),
        ("SELECT record_id FROM business_records WHERE collection='ctox_harness_events' AND deleted=0 AND json_extract(payload_json,'$.created_at_ms')>=1 ORDER BY json_extract(payload_json,'$.created_at_ms')", "idx_cockpit_event_created"),
    ] {
        let details = conn.prepare(&format!("EXPLAIN QUERY PLAN {query}"))?
            .query_map([], |r| r.get::<_, String>(3))?
            .collect::<rusqlite::Result<Vec<_>>>()?.join("\n");
        assert!(details.contains(index), "{index}: {details}");
    }
    Ok(())
}

#[test]
fn crew_native_projection_indexes_and_payloads_are_bounded() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("ALTER TABLE communication_routing_state ADD COLUMN leased_at TEXT;")?;
    crate::crew::ensure_schema(&conn)?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_crew(root.path(), &conn, &mut writer)?;
    let member = record(root.path(), "ctox_crew_members", "crew-milo")?;
    assert_eq!(member["name"], "Milo");
    assert_eq!(member["state"], "home");
    assert!(member["updated_at_ms"].is_number());
    assert!(member["soul"].is_object());
    let conn = store::open_store(root.path())?;
    for (query,index) in [
        ("SELECT record_id FROM business_records WHERE collection='ctox_crew_members' AND deleted=0 AND json_extract(payload_json,'$.archived')=0 ORDER BY json_extract(payload_json,'$.state'),record_id", "idx_crew_projection_member_state"),
        ("SELECT record_id FROM business_records WHERE collection='ctox_crew_learnings' AND deleted=0 AND json_extract(payload_json,'$.member_id')='crew-milo' ORDER BY json_extract(payload_json,'$.created_at_ms'),record_id", "idx_crew_projection_learning_time"),
        ("SELECT record_id FROM business_records WHERE collection='ctox_crew_learnings' AND deleted=0 AND json_extract(payload_json,'$.member_id')='crew-milo' ORDER BY json_extract(payload_json,'$.confirmed_by_owner'),record_id", "idx_crew_projection_learning_confirmed"),
        ("SELECT record_id FROM business_records WHERE collection IN ('ctox_crew_members','ctox_crew_learnings') AND deleted=0 ORDER BY collection,json_extract(payload_json,'$.updated_at_ms'),record_id", "idx_crew_projection_updated"),
    ] {
        let plan=conn.prepare(&format!("EXPLAIN QUERY PLAN {query}"))?.query_map([],|r|r.get::<_,String>(3))?.collect::<rusqlite::Result<Vec<_>>>()?.join("\n");
        assert!(plan.contains(index), "{plan}"); assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
    Ok(())
}

#[test]
fn crew_attempt_projects_timesheet_stats_and_learnings_once() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch(
        "ALTER TABLE communication_routing_state ADD COLUMN leased_at TEXT;
        ALTER TABLE worker_attempt_finalizations ADD COLUMN reply_text TEXT NOT NULL DEFAULT '';",
    )?;
    crate::crew::ensure_schema(&conn)?;
    conn.execute("INSERT INTO communication_routing_state(message_key,route_status,updated_at,crew_member_id) VALUES('task','leased','2026-09-05T12:00:00Z','crew-milo')", [])?;
    conn.execute("INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at) VALUES('attempt','task','crew-milo','2026-09-05T12:00:00Z')", [])?;
    let reply = json!({"crew_retrospective":{"retrospective":"Schema geprüft.","learnings":[{"text":"Schema vor dem Import prüfen.","kind":"insight","scope":{}}]}}).to_string();
    conn.execute("INSERT INTO worker_attempt_finalizations VALUES('attempt','work','succeeded','success','1788609600000','1788609660000','1788609660000',NULL,0,?1)", [&reply])?;
    conn.execute("INSERT INTO ctox_harness_flow_events VALUES('event','worker.phase','Review','','task',NULL,1,?1,'2026-09-05T12:01:00Z')", [json!({"attempt_id":"attempt","review":{"disposition":"approved"}}).to_string()])?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    for _ in 0..2 {
        project_runs(root.path(), &conn, &mut writer)?;
        project_crew(root.path(), &conn, &mut writer)?;
    }
    let run = record(root.path(), "ctox_runs", "attempt")?;
    assert_eq!(run["crew_member_id"], "crew-milo");
    assert_eq!(run["retrospective"], "Schema geprüft.");
    assert!(run["finished_at_ms"].is_number());
    let member = record(root.path(), "ctox_crew_members", "crew-milo")?;
    assert_eq!(member["state"], "on_duty");
    assert_eq!(member["stats"]["tasks_total"], 1);
    assert_eq!(member["stats"]["review_passed"], 1);
    // Learnings wait on the attempt for the learner; the learner writes them
    // into the member's memory (LCM anchors) and the projection carries it.
    let (learning_json, due): (String, i64) = conn.query_row(
        "SELECT learning_json,learning_due FROM crew_attempts WHERE attempt_id='attempt'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert!(learning_json.contains("Schema vor dem Import prüfen."));
    assert_eq!(due, 1);
    // The learner itself is covered in crew::lifecycle_tests on a full LCM
    // store; this fixture only carries the projection tables. The member
    // document still declares its (empty) memory and derived field of work.
    assert_eq!(member["memory"]["anchor_count"], 0);
    assert!(member["domain"].is_array());
    // Legacy learning rows keep their projection until they are migrated.
    let learning = "legacy-learning".to_string();
    conn.execute("INSERT INTO crew_member_learnings(id,member_id,text,normalized_text,kind,scope_json,evidence_run_id,created_at) VALUES(?1,'crew-milo','Alt','alt','pitfall','{}','attempt','2026-09-05T12:02:00Z')", [&learning])?;
    project_crew(root.path(), &conn, &mut writer)?;
    assert_eq!(
        record(root.path(), "ctox_crew_learnings", &learning)?["confirmed_by_owner"],
        false
    );
    conn.execute(
        "UPDATE crew_member_learnings SET confirmed_by_owner=1 WHERE id=?1",
        [&learning],
    )?;
    project_crew(root.path(), &conn, &mut writer)?;
    assert_eq!(
        record(root.path(), "ctox_crew_learnings", &learning)?["confirmed_by_owner"],
        true
    );
    conn.execute("DELETE FROM crew_member_learnings WHERE id=?1", [&learning])?;
    project_crew(root.path(), &conn, &mut writer)?;
    let deleted: bool = store::open_store(root.path())?.query_row("SELECT deleted FROM business_records WHERE collection='ctox_crew_learnings' AND record_id=?1", [&learning], |r|r.get(0))?;
    assert!(deleted);
    conn.execute(
        "UPDATE crew_members SET updated_at='broken' WHERE id='crew-pico'",
        [],
    )?;
    conn.execute("INSERT INTO crew_member_learnings(id,member_id,text,normalized_text,kind,scope_json,evidence_run_id,created_at) VALUES('bad-time','crew-milo','Prüfen','prüfen','pitfall','{}','attempt','broken')", [])?;
    project_crew(root.path(), &conn, &mut writer)?;
    let count: i64 = store::open_store(root.path())?.query_row("SELECT COUNT(*) FROM business_records WHERE collection='ctox_crew_learnings' AND record_id='bad-time'", [], |r|r.get(0))?;
    assert_eq!(count, 0);
    assert!(record(root.path(), "ctox_crew_members", "crew-pico")?["updated_at_ms"].is_number());
    Ok(())
}

#[test]
fn keyset_pages_project_every_active_task_and_older_active_run() -> Result<()> {
    let (root, conn) = setup()?;
    for index in 0..PROJECTION_PAGE_SIZE + 1 {
        let id = format!("active-{index:04}");
        conn.execute(
            "INSERT INTO communication_routing_state VALUES(?1,'pending','2020-01-01T00:00:00Z')",
            [&id],
        )?;
        conn.execute("INSERT INTO worker_attempt_finalizations VALUES(?1,'work','failed','failed','1','2','2',NULL,1)", [&id])?;
        conn.execute("INSERT INTO ctox_harness_flow_events VALUES(?1,'worker.phase','Phase','',?1,NULL,1,?2,'2020-01-01T00:00:00Z')",
            params![id, json!({"attempt_id":id,"step_position":null}).to_string()])?;
    }
    for index in 0..501 {
        conn.execute("INSERT INTO worker_attempt_finalizations VALUES(?1,'work','succeeded','success','3','4','4',NULL,0)", [format!("terminal-{index:04}")])?;
    }
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_events(root.path(), &conn, &mut writer)?;
    project_runs(root.path(), &conn, &mut writer)?;
    let business = store::open_store(root.path())?;
    let events: i64 = business.query_row("SELECT COUNT(*) FROM business_records WHERE collection='ctox_harness_events' AND deleted=0", [], |r| r.get(0))?;
    let runs: i64 = business.query_row(
        "SELECT COUNT(*) FROM business_records WHERE collection='ctox_runs' AND deleted=0",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(events, PROJECTION_PAGE_SIZE + 1);
    assert_eq!(runs, 500 + PROJECTION_PAGE_SIZE + 1);
    assert!(record(root.path(), "ctox_runs", "terminal-0000").is_err());
    assert!(record(root.path(), "ctox_runs", "active-0000").is_ok());
    Ok(())
}

#[test]
fn retention_keeps_active_tasks_and_tombstones_all_three_projection_surfaces() -> Result<()> {
    let (root, conn) = setup()?;
    crate::inference::runtime_env::set_runtime_env_value(
        root.path(),
        super::super::QUEUE_RETENTION_KEY,
        "3",
    )?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    for index in 0..7 {
        let id = format!("task-{index}");
        let status = if index < 5 { "handled" } else { "pending" };
        conn.execute(
            "INSERT INTO communication_routing_state VALUES(?1,?2,?3)",
            params![id, status, format!("2020-01-0{}T00:00:00Z", index + 1)],
        )?;
        writer.upsert_source_projection(
            "ctox_queue_tasks",
            &id,
            index,
            json!({"id":id,"route_status":status}),
        )?;
    }
    conn.execute(
        "INSERT INTO communication_routing_state VALUES('recent','handled',?1)",
        [Utc::now().to_rfc3339()],
    )?;
    writer.upsert_source_projection(
        "ctox_queue_tasks",
        "recent",
        0,
        json!({"id":"recent","route_status":"handled"}),
    )?;
    for index in 0..210 {
        let id = format!("event-{index:03}");
        writer.upsert_source_projection(
            "ctox_harness_events",
            &id,
            index,
            json!({"id":id,"task_id":"task-5","created_at_ms":index}),
        )?;
    }
    for (id, task) in [("expired", "task-0"), ("recent-event", "recent")] {
        writer.upsert_source_projection(
            "ctox_harness_events",
            id,
            0,
            json!({"id":id,"task_id":task,"created_at_ms":0}),
        )?;
    }
    for index in 0..503 {
        let id = format!("run-{index:03}");
        let task = if index == 0 { "task-5" } else { "task-0" };
        writer.upsert_source_projection(
            "ctox_runs",
            &id,
            index,
            json!({"id":id,"task_id":task,"finished_at_ms":index}),
        )?;
    }
    retain(
        root.path(),
        &conn,
        &mut writer,
        Utc::now().timestamp_millis(),
    )?;
    let business = store::open_store(root.path())?;
    for (collection, expected) in [
        ("ctox_queue_tasks", 5),
        ("ctox_harness_events", 201),
        ("ctox_runs", 501),
    ] {
        let actual: i64 = business.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection=?1 AND deleted=0",
            [collection],
            |r| r.get(0),
        )?;
        assert_eq!(actual, expected, "{collection}");
    }
    assert_eq!(
        record(root.path(), "ctox_harness_events", "expired")?["_deleted"],
        true
    );
    assert_eq!(
        record(root.path(), "ctox_runs", "run-000")?["_deleted"],
        false
    );
    let rxdb = Connection::open(store::rxdb_store_path(root.path()))?;
    let deleted: bool = rxdb.query_row(
        "SELECT deleted FROM ctox_business_os__ctox_harness_events__v0 WHERE id='expired'",
        [],
        |r| r.get(0),
    )?;
    assert!(deleted, "retention tombstones replicate");
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM communication_routing_state",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        8,
        "durable history is untouched"
    );
    Ok(())
}

#[test]
fn events_are_bounded_redacted_and_idempotent_and_terminal_tasks_do_not_grow() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute(
        "INSERT INTO communication_routing_state VALUES('active','leased','2026-01-01T00:00:00Z')",
        [],
    )?;
    for index in 0..210 {
        conn.execute("INSERT INTO ctox_harness_flow_events VALUES(?1,'worker.tool_started','Terminal','private tool output','active',NULL,2,?2,?3)",params![format!("event-{index:03}"),json!({"attempt_id":"attempt","attempt":2,"command_id":"command","tool":{"name":"Terminal","type":"exec_command","call_id":format!("call-{index}"),"arguments":{"secret":"must not replicate"}},"step_position":1}).to_string(),format!("2026-01-01T00:{:02}:{:02}Z",index/60,index%60)])?;
    }
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_events(root.path(), &conn, &mut writer)?;
    let before = record(root.path(), "ctox_harness_events", "event-209")?;
    assert_eq!(before["kind"], "tool_started");
    assert_eq!(before["attempt"], 2);
    assert_eq!(before["step_position"], 1);
    assert!(!before.to_string().contains("must not replicate"));
    assert!(!before.to_string().contains("private tool output"));
    project_events(root.path(), &conn, &mut writer)?;
    assert_eq!(
        before,
        record(root.path(), "ctox_harness_events", "event-209")?,
        "unchanged replay creates no new revision"
    );
    assert_eq!(store::open_store(root.path())?.query_row("SELECT COUNT(*) FROM business_records WHERE collection='ctox_harness_events' AND deleted=0",[],|r|r.get::<_,i64>(0))?,200);
    conn.execute(
        "UPDATE communication_routing_state SET route_status='handled'",
        [],
    )?;
    conn.execute("INSERT INTO ctox_harness_flow_events SELECT 'after-terminal',event_kind,title,body_text,message_key,work_id,attempt_index,json_set(metadata_json,'$.cockpit_eligible',json('false')),'2026-01-02T00:00:00Z' FROM ctox_harness_flow_events LIMIT 1",[])?;
    project_events(root.path(), &conn, &mut writer)?;
    assert!(record(root.path(), "ctox_harness_events", "after-terminal").is_err());
    conn.execute(
        "UPDATE communication_routing_state SET route_status='pending'",
        [],
    )?;
    project_events(root.path(), &conn, &mut writer)?;
    assert!(
        record(root.path(), "ctox_harness_events", "after-terminal").is_err(),
        "retry must not publish events emitted after terminalization"
    );
    Ok(())
}

#[test]
fn short_completed_turn_replays_only_events_eligible_at_emission() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute(
        "INSERT INTO communication_routing_state VALUES('quick','handled',?1)",
        [Utc::now().to_rfc3339()],
    )?;
    for (id, eligible) in [("before-finish", true), ("after-finish", false)] {
        conn.execute("INSERT INTO ctox_harness_flow_events VALUES(?1,'worker.turn_completed','Done','','quick',NULL,1,?2,?3)",params![id,json!({"cockpit_eligible":eligible}).to_string(),Utc::now().to_rfc3339()])?;
    }
    project_events(
        root.path(),
        &conn,
        &mut BusinessProjectionWriter::open(root.path())?,
    )?;
    assert!(record(root.path(), "ctox_harness_events", "before-finish").is_ok());
    assert!(record(root.path(), "ctox_harness_events", "after-finish").is_err());
    Ok(())
}

#[test]
fn runs_join_real_turn_ids_and_refresh_late_costs_without_double_counting() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute(
        "INSERT INTO communication_routing_state VALUES('task','handled','2026-01-01T00:00:03Z')",
        [],
    )?;
    conn.execute("INSERT INTO worker_attempt_finalizations VALUES('attempt','work','succeeded','success','1767225602000','1767225603000','1767225603000',NULL,0)",[])?;
    conn.execute("INSERT INTO ctox_harness_flow_events VALUES('link','worker.tool_started','Terminal','','task',NULL,1,?1,'2026-01-01T00:00:00Z')",[json!({"attempt_id":"attempt","command_id":"command","turn_id":"actual-turn","tool":{"call_id":"call-1"}}).to_string()])?;
    conn.execute("INSERT INTO api_model_cost_events VALUES('provider','model','actual-turn',100,20,10,4,'2026-01-01')",[])?;
    conn.execute("INSERT INTO api_model_cost_events VALUES('provider','model','attempt',9999,0,9999,0,'2026-01-01')",[])?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_runs(root.path(), &conn, &mut writer)?;
    let first = record(root.path(), "ctox_runs", "attempt")?;
    assert_eq!(first["metrics"]["input_tokens"], 100);
    assert_eq!(first["metrics"]["elapsed_ms"], 3000);
    assert_eq!(first["metrics"]["tool_calls"], 1);
    assert!(
        first["metrics"]["cost_usd"].is_null(),
        "unpriced usage is unknown, not free"
    );
    conn.execute(
        "INSERT INTO api_model_price_rates VALUES('provider','model',2,1,4,'2025-01-01')",
        [],
    )?;
    conn.execute("INSERT INTO api_model_cost_events VALUES('provider','model','actual-turn',50,0,5,2,'2026-01-01')",[])?;
    project_runs(root.path(), &conn, &mut writer)?;
    let late = record(root.path(), "ctox_runs", "attempt")?;
    assert_eq!(late["metrics"]["input_tokens"], 150);
    assert_eq!(late["metrics"]["reasoning_tokens"], 6);
    assert!((late["metrics"]["cost_usd"].as_f64().unwrap() - 0.00034).abs() < 1e-10);
    project_runs(root.path(), &conn, &mut writer)?;
    assert_eq!(late, record(root.path(), "ctox_runs", "attempt")?);
    Ok(())
}

#[test]
fn projection_delivery_recovers_when_rxdb_collection_appears_after_writer_open() -> Result<()> {
    let (root, _) = setup()?;
    let rxdb = Connection::open(store::rxdb_store_path(root.path()))?;
    rxdb.execute_batch("DROP TABLE ctox_business_os__ctox_harness_events__v0;")?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    let payload =
        json!({"id":"late","kind":"phase","task_id":"task","created_at_ms":1,"updated_at_ms":1});
    writer.upsert_source_projection("ctox_harness_events", "late", 1, payload.clone())?;
    assert!(writer.payloads.is_empty());
    rxdb.execute_batch("CREATE TABLE ctox_business_os__ctox_harness_events__v0(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER DEFAULT 0,lastWriteTime REAL DEFAULT 0,data TEXT NOT NULL);")?;
    writer.upsert_source_projection("ctox_harness_events", "late", 1, payload)?;
    assert_eq!(
        rxdb.query_row(
            "SELECT COUNT(*) FROM ctox_business_os__ctox_harness_events__v0 WHERE id='late'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

#[test]
fn worker_snapshot_hooks_publish_start_and_graceful_stop() -> Result<()> {
    let (root, _) = setup()?;
    publish_worker_snapshot(
        root.path(),
        WorkerSnapshot {
            service_running: true,
            busy: true,
            worker_active_count: 1,
            active_task_ids: vec!["hook-task".into()],
            boot_id: "hook-boot".into(),
            ..Default::default()
        },
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if record(root.path(), "ctox_harness_status", "harness")
            .ok()
            .is_some_and(|r| r["busy"] == true && r["active_task_ids"] == json!(["hook-task"]))
        {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "worker start snapshot was not projected"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    publish_service_stopped(root.path(), "hook-boot".into());
    assert_eq!(
        record(root.path(), "ctox_harness_status", "harness")?["service_running"],
        false
    );
    Ok(())
}

#[test]
fn status_tracks_start_stop_and_persistent_pause_with_no_change_churn() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute(
        "INSERT INTO communication_routing_state VALUES('task','leased',?1)",
        [Utc::now().to_rfc3339()],
    )?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    let active = WorkerSnapshot {
        service_running: true,
        busy: true,
        worker_active_count: 1,
        worker_phase: Some("model".into()),
        active_task_ids: vec!["task".into()],
        boot_id: "boot".into(),
        ..Default::default()
    };
    project_status(root.path(), &conn, &mut writer, &active)?;
    let before = record(root.path(), "ctox_harness_status", "harness")?;
    project_status(root.path(), &conn, &mut writer, &active)?;
    assert_eq!(
        before,
        record(root.path(), "ctox_harness_status", "harness")?
    );
    crate::inference::runtime_env::set_runtime_env_value(
        root.path(),
        super::super::PAUSE_KEY,
        r#"{"paused":true,"reason":"operator"}"#,
    )?;
    project_status(
        root.path(),
        &conn,
        &mut writer,
        &WorkerSnapshot {
            boot_id: "boot".into(),
            ..Default::default()
        },
    )?;
    let stopped = record(root.path(), "ctox_harness_status", "harness")?;
    assert_eq!(stopped["worker_active_count"], 0);
    assert_eq!(stopped["service_running"], false);
    assert_eq!(stopped["paused"], true);
    assert_eq!(stopped["pause_reason"], "operator");
    assert!(stopped["active_task_ids"].as_array().unwrap().is_empty());
    assert!(stopped["active_crew_member_id"].is_null());
    Ok(())
}
#[test]
fn repeated_worker_snapshots_are_counted_without_repeated_wakes() {
    let root = Path::new("fixture");
    let mut latest = BTreeMap::new();
    assert!(record_snapshot(
        &mut latest,
        root,
        WorkerSnapshot::default()
    ));
    for _ in 0..1000 {
        assert!(!record_snapshot(
            &mut latest,
            root,
            WorkerSnapshot::default()
        ));
    }
    assert!(record_snapshot(
        &mut latest,
        root,
        WorkerSnapshot {
            busy: true,
            ..Default::default()
        }
    ));
    assert_eq!(latest[root].publications, 1002);
    assert_eq!(latest[root].changes, 2);
    assert!(latest[root].snapshot.busy);
}

#[test]
fn only_visible_flow_and_chat_events_wake_the_pump() {
    for kind in [
        "router.idle",
        "queue.polled",
        "unrelated",
        "worker.unrecognized",
    ] {
        assert_eq!(flow_refresh_flags(kind), 0, "{kind}");
    }
    for kind in [
        "worker.tool_started",
        "worker.tool_completed",
        "worker.thinking",
        "crew.memory_read",
        "crew.learning",
    ] {
        assert_eq!(flow_refresh_flags(kind), EVENTS, "{kind}");
    }
    assert_eq!(flow_refresh_flags("crew_selected"), EVENTS | STATUS | QUEUE);
    assert_eq!(flow_refresh_flags("cockpit.review"), EVENTS | CHAT);
    assert_eq!(flow_refresh_flags("worker.plan_updated"), EVENTS | CHAT);
}

#[test]
fn incremental_events_skip_unchanged_tasks_and_replay_repairs() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch("INSERT INTO communication_routing_state VALUES('a','leased','2026-01-01');
        INSERT INTO communication_routing_state VALUES('b','leased','2026-01-01');
        INSERT INTO ctox_harness_flow_events VALUES('a0','worker.phase','A','','a',NULL,NULL,'{}','2026-01-02T00:00:00Z');
        INSERT INTO ctox_harness_flow_events VALUES('b0','worker.phase','B','','b',NULL,NULL,'{}','2026-01-02T00:00:00Z');")?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_events_since(root.path(), &conn, &mut writer, false)?;
    assert_eq!(writer.event_cursor, Some(2));
    // An in-place source repair stays untouched by the incremental hot path.
    conn.execute(
        "UPDATE ctox_harness_flow_events SET title='Repaired A' WHERE event_id='a0'",
        [],
    )?;
    project_events_since(root.path(), &conn, &mut writer, false)?;
    assert_eq!(
        record(root.path(), "ctox_harness_events", "a0")?["title"],
        "A"
    );
    // Backdated insertion must be seen; created_at cannot be the cursor.
    conn.execute_batch("INSERT INTO ctox_harness_flow_events VALUES('b1','worker.phase','Late B','','b',NULL,NULL,'{}','2026-01-01T00:00:00Z');")?;
    project_events_since(root.path(), &conn, &mut writer, false)?;
    assert_eq!(
        record(root.path(), "ctox_harness_events", "b1")?["title"],
        "Late B"
    );
    assert_eq!(writer.event_cursor, Some(3));
    assert_eq!(
        record(root.path(), "ctox_harness_events", "a0")?["title"],
        "A"
    );
    project_events_since(root.path(), &conn, &mut writer, true)?;
    assert_eq!(
        record(root.path(), "ctox_harness_events", "a0")?["title"],
        "Repaired A"
    );
    // Reopening the pump starts from durable retained history.
    let mut restarted = BusinessProjectionWriter::open(root.path())?;
    assert_eq!(restarted.event_cursor, None);
    project_events_since(root.path(), &conn, &mut restarted, false)?;
    assert_eq!(restarted.event_cursor, Some(3));
    Ok(())
}

#[test]
fn event_cursor_waits_for_delivery_and_task_cap_stays_immediate() -> Result<()> {
    let (root, conn) = setup()?;
    conn.execute_batch(
        "INSERT INTO communication_routing_state VALUES('task','leased','2026-01-01');",
    )?;
    for n in 0..201 {
        conn.execute("INSERT INTO ctox_harness_flow_events VALUES(?1,'worker.phase','Phase','','task',NULL,NULL,'{}',?2)",
            params![format!("event-{n:03}"), format!("2026-01-01T00:{:02}:{:02}Z", n / 60, n % 60)])?;
    }
    let rxdb = Connection::open(store::rxdb_store_path(root.path()))?;
    rxdb.execute_batch(
        "ALTER TABLE ctox_business_os__ctox_harness_events__v0 RENAME TO hidden_events;",
    )?;
    let mut writer = BusinessProjectionWriter::open(root.path())?;
    project_events_since(root.path(), &conn, &mut writer, false)?;
    assert_eq!(writer.event_cursor, None);
    rxdb.execute_batch(
        "ALTER TABLE hidden_events RENAME TO ctox_business_os__ctox_harness_events__v0;",
    )?;
    project_events_since(root.path(), &conn, &mut writer, false)?;
    assert_eq!(writer.event_cursor, Some(201));
    conn.execute_batch("INSERT INTO ctox_harness_flow_events VALUES('new','worker.phase','New','','task',NULL,NULL,'{}','2026-01-01T01:00:00Z');")?;
    project_events_since(root.path(), &conn, &mut writer, false)?;
    assert_eq!(writer.inner.source_connection().query_row(
        "SELECT COUNT(*) FROM business_records WHERE collection='ctox_harness_events' AND deleted=0", [], |r| r.get::<_, i64>(0))?, 200);
    assert_eq!(writer.inner.source_connection().query_row(
        "SELECT deleted FROM business_records WHERE collection='ctox_harness_events' AND record_id='event-001'", [], |r| r.get::<_, i64>(0))?, 1);
    assert_eq!(
        record(root.path(), "ctox_harness_events", "new")?["title"],
        "New"
    );
    Ok(())
}

#[test]
fn changed_event_query_and_task_retention_use_bounded_indexes() -> Result<()> {
    let (root, _fixture) = setup()?;
    let conn = core(root.path())?;
    let writer = BusinessProjectionWriter::open(root.path())?;
    let plan = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {CHANGED_EVENT_TASKS_SQL}"))?
        .query_map(params![100, 101, "", 128], |row| row.get::<_, String>(3))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .join("\n");
    assert!(
        plan.contains("SEARCH e USING INTEGER PRIMARY KEY (rowid>? AND rowid<?)"),
        "{plan}"
    );
    assert!(!plan.contains("SCAN e"), "{plan}");
    let plan = writer
        .inner
        .source_connection()
        .prepare(&format!("EXPLAIN QUERY PLAN {EXCESS_TASK_EVENTS_SQL}"))?
        .query_map(["task"], |row| row.get::<_, String>(3))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .join("\n");
    assert!(plan.contains("idx_cockpit_event_task_time"), "{plan}");
    assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    Ok(())
}

#[test]
fn idle_event_query_cost_does_not_grow_with_retained_history() -> Result<()> {
    use rusqlite::StatementStatus;
    let (root, fixture) = setup()?;
    fixture.execute_batch("BEGIN;
        WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<1000)
        INSERT INTO communication_routing_state SELECT printf('task-%04d',i),'leased','2026-01-01' FROM n;
        WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<50)
        INSERT INTO ctox_harness_flow_events
        SELECT r.message_key||'-'||n.i,'worker.phase','Phase','',r.message_key,NULL,NULL,'{}','2026-01-01T00:00:00Z'
        FROM communication_routing_state r CROSS JOIN n;
        COMMIT;")?;
    let conn = core(root.path())?;
    let high_water: i64 =
        conn.query_row("SELECT MAX(rowid) FROM ctox_harness_flow_events", [], |r| {
            r.get(0)
        })?;
    assert_eq!(high_water, 50_000);
    let mut incremental = conn.prepare(CHANGED_EVENT_TASKS_SQL)?;
    let unchanged = incremental
        .query_map(params![high_water, high_water, "", 128], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assert!(unchanged.is_empty());
    let idle_steps = incremental.reset_status(StatementStatus::VmStep);
    assert!(
        idle_steps < 100,
        "idle query visited history: {idle_steps} VM steps"
    );
    let changed = incremental
        .query_map(params![high_water - 1, high_water, "", 128], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assert_eq!(changed.len(), 1);
    let changed_steps = incremental.get_status(StatementStatus::VmStep);
    assert!(
        changed_steps < 150,
        "single insertion visited history: {changed_steps} VM steps"
    );
    // Original per-wake task enumeration alone, before its per-task 200-event
    // reads and JSON/plan projection. Count VM work, not flaky wall-clock time.
    let mut baseline = conn.prepare(
        "SELECT message_key FROM communication_routing_state
        WHERE message_key > '' AND (route_status NOT IN ('handled','failed','cancelled')
        OR julianday(updated_at)>=julianday('now','-1 day')) ORDER BY message_key",
    )?;
    assert_eq!(
        baseline.query_map([], |r| r.get::<_, String>(0))?.count(),
        1000
    );
    let baseline_steps = baseline.get_status(StatementStatus::VmStep);
    assert!(baseline_steps > 100 * idle_steps);
    eprintln!("cockpit VM benchmark: 1000 tasks/50000 events; baseline task enumeration={baseline_steps}, incremental idle={idle_steps}, one inserted row={changed_steps}");
    Ok(())
}
