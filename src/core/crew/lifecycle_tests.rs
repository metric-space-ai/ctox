use super::*;
use serde_json::json;

fn leased() -> Result<(tempfile::TempDir, Connection, String)> {
    let root = tempfile::tempdir()?;
    let task = crate::mission::channels::create_queue_task(
        root.path(),
        crate::mission::channels::QueueTaskCreateRequest {
            title: "Lifecycle fixture".into(),
            prompt: "Inspect data".into(),
            thread_key: "crew-thread".into(),
            workspace_root: None,
            priority: "normal".into(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )?;
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    conn.execute("UPDATE communication_routing_state SET route_status='leased',lease_owner='fixture' WHERE message_key=?1", [&task.message_key])?;
    Ok((root, conn, task.message_key))
}

#[test]
fn crew_migration_keeps_flow_ledger_lazy_until_admission() -> Result<()> {
    let (root, conn, task) = leased()?;
    assert!(!conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='ctox_harness_flow_events')",
        [],
        |r| r.get::<_, bool>(0)
    )?);
    prepare_attempt(
        root.path(),
        &[task],
        "fixture",
        "admitted",
        None,
        &json!({}),
        None,
        "Inspect",
        None,
    )?;
    assert!(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='idx_crew_selection_event_attempt')",
        [],
        |r| r.get::<_, bool>(0)
    )?);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM ctox_harness_flow_events WHERE event_kind='crew_selected'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

#[test]
fn crew_unavailable_continues_without_retry_or_identity_and_warns_once() -> Result<()> {
    let (root, conn, task) = leased()?;
    conn.execute("UPDATE crew_members SET archived=1", [])?;
    conn.execute("UPDATE communication_routing_state SET crew_member_id='crew-nori',crew_assigned_member_id='crew-milo' WHERE message_key=?1", [&task])?;
    for attempt in ["a", "b"] {
        assert!(prepare_attempt_or_continue(
            root.path(),
            &[task.clone()],
            "fixture",
            attempt,
            None,
            &json!({}),
            None,
            "Inspect",
            None,
        )
        .is_none());
    }
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM crew_attempts", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    let state:(String,Option<String>,i64,Option<String>)=conn.query_row(
        "SELECT route_status,crew_member_id,failure_attempt_count,retry_not_before FROM communication_routing_state WHERE message_key=?1",
        [&task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    assert_eq!(state, ("leased".into(), None, 0, None));
    assert_eq!(
        conn.query_row(
            "SELECT crew_assigned_member_id FROM communication_routing_state WHERE message_key=?1",
            [&task],
            |r| r.get::<_, String>(0)
        )?,
        "crew-milo"
    );
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events WHERE event_kind='crew_selection_unavailable'", [], |r|r.get::<_,i64>(0))?,1);
    assert!(selection_last_error(root.path())
        .unwrap()
        .contains("no active crew member"));
    Ok(())
}

#[test]
fn crew_corrupt_profile_is_skipped_and_other_members_can_work() -> Result<()> {
    let (root, conn, task) = leased()?;
    conn.execute(
        "UPDATE crew_members SET soul_json='{broken' WHERE id='crew-lumi'",
        [],
    )?;
    for attempt in ["a", "b"] {
        assert!(prepare_attempt_or_continue(
            root.path(),
            &[task.clone()],
            "fixture",
            attempt,
            None,
            &json!({}),
            None,
            "Inspect",
            None,
        )
        .is_some());
    }
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM crew_attempts WHERE member_id='crew-lumi'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events WHERE event_kind='crew_selection_unavailable'", [], |r|r.get::<_,i64>(0))?,1);
    assert!(selection_last_error(root.path())
        .unwrap()
        .contains("invalid crew member data"));
    Ok(())
}

#[test]
fn crew_retry_scores_again_and_consumes_assignment_once() -> Result<()> {
    let (root, conn, task) = leased()?;
    conn.execute(
        "UPDATE crew_members SET archived=1 WHERE id NOT IN ('crew-milo','crew-nori')",
        [],
    )?;
    conn.execute("UPDATE communication_routing_state SET crew_assigned_member_id='crew-milo' WHERE message_key=?1", [&task])?;
    let metadata = json!({"module_id":"sample"});
    prepare_attempt(
        root.path(),
        &[task.clone()],
        "fixture",
        "first",
        Some("crew-thread"),
        &metadata,
        None,
        "Inspect",
        None,
    )?;
    let columns:(String,Option<String>)=conn.query_row("SELECT crew_member_id,crew_assigned_member_id FROM communication_routing_state WHERE message_key=?1", [&task],|r|Ok((r.get(0)?,r.get(1)?)))?;
    assert_eq!(columns, ("crew-milo".into(), None));
    finalize_attempt(
        &conn,
        "first",
        "failed",
        Some(false),
        &chrono::Utc::now().to_rfc3339(),
        Some(1),
        "",
        None,
    )?;
    // Remove the inactivity tie-break: only the recent-failure score can decide.
    conn.execute(
        "UPDATE crew_members SET stats_json=json_set(stats_json,'$.last_active_at',NULL)",
        [],
    )?;
    prepare_attempt(
        root.path(),
        &[task.clone()],
        "fixture",
        "retry",
        Some("crew-thread"),
        &metadata,
        None,
        "Inspect",
        None,
    )?;
    let second: String = conn.query_row(
        "SELECT member_id FROM crew_attempts WHERE attempt_id='retry'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(second, "crew-nori");
    for (attempt, prefix) in [("first", "assigned:"), ("retry", "selected:")] {
        let reason: String = conn.query_row(
            "SELECT selection_reason FROM crew_attempts WHERE attempt_id=?1",
            [attempt],
            |r| r.get(0),
        )?;
        let event:String=conn.query_row("SELECT title FROM ctox_harness_flow_events WHERE event_kind='crew_selected' AND json_extract(metadata_json,'$.attempt_id')=?1",[attempt],|r|r.get(0))?;
        assert!(reason.starts_with(prefix));
        assert_eq!(event, reason);
    }
    Ok(())
}

#[test]
fn crew_continuity_ignores_never_started_attempts() -> Result<()> {
    let (root, conn, task) = leased()?;
    conn.execute("INSERT INTO crew_attempts(attempt_id,task_id,member_id,thread_key,selected_at) VALUES('orphan','other','crew-pico','crew-thread',?1)",[chrono::Utc::now().to_rfc3339()])?;
    let block = prepare_attempt(
        root.path(),
        &[task],
        "fixture",
        "admitted",
        Some("crew-thread"),
        &json!({}),
        None,
        "Inspect",
        None,
    )?
    .unwrap();
    assert_ne!(block.member_name, "Pico");
    Ok(())
}

#[test]
fn crew_continuity_kind_and_reason_survive_event_repair() -> Result<()> {
    let (root, conn, task) = leased()?;
    conn.execute("INSERT INTO crew_attempts(attempt_id,task_id,member_id,thread_key,selected_at,started_at) VALUES('prior','other','crew-pico','crew-thread',?1,?1)", [chrono::Utc::now().to_rfc3339()])?;
    prepare_attempt(
        root.path(),
        &[task],
        "fixture",
        "continued",
        Some("crew-thread"),
        &json!({}),
        None,
        "Inspect",
        None,
    )?;
    let audit = || -> Result<(String, String)> {
        Ok(conn.query_row("SELECT title,json_extract(metadata_json,'$.selection_kind') FROM ctox_harness_flow_events WHERE event_kind='crew_selected' AND json_extract(metadata_json,'$.attempt_id')='continued'", [], |r|Ok((r.get(0)?,r.get(1)?)))?)
    };
    let before = audit()?;
    assert!(before.0.starts_with("continuity:"));
    assert_eq!(before.1, "continuity");
    conn.execute(
        "DELETE FROM ctox_harness_flow_events WHERE event_kind='crew_selected'",
        [],
    )?;
    repair_selection_events(root.path(), &conn)?;
    assert_eq!(audit()?, before);
    Ok(())
}

#[test]
fn crew_retention_is_bounded_preserves_active_and_explains_indexes() -> Result<()> {
    let conn = super::tests::fixture();
    conn.execute("INSERT INTO communication_routing_state(message_key,route_status) VALUES('active','leased')", [])?;
    for i in 0..640 {
        conn.execute("INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at,started_at,finalized_at)
            VALUES(?1,'closed','crew-milo',?1,?1,?1)",[format!("2026-01-{i:04}")])?;
    }
    conn.execute(
        "INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at,finalized_at)
        VALUES('active-run','active','crew-milo','2025','2025')",
        [],
    )?;
    conn.execute(
        "INSERT INTO crew_attempts(attempt_id,task_id,member_id,selected_at)
        VALUES('orphan','closed','crew-milo','2020-01-01T00:00:00Z')",
        [],
    )?;
    let now = chrono::Utc::now().timestamp_millis();
    retain_attempts(&conn, now)?;
    // First pass removes at most 128 completed rows plus the one expired orphan.
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM crew_attempts", [], |r| r
            .get::<_, i64>(0))?,
        513
    );
    retain_attempts(&conn, now)?;
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM crew_attempts", [], |r| r
            .get::<_, i64>(0))?,
        501
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM crew_attempts WHERE attempt_id='active-run'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    let plan=conn.prepare("EXPLAIN QUERY PLAN SELECT attempt_id FROM crew_attempts WHERE started_at IS NULL AND finalized_at IS NULL AND selected_at<'2026' ORDER BY selected_at,attempt_id LIMIT 128")?
        .query_map([],|r|r.get::<_,String>(3))?.collect::<rusqlite::Result<Vec<_>>>()?.join("\n");
    assert!(plan.contains("idx_crew_attempt_unstarted"), "{plan}");
    assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    Ok(())
}

#[test]
fn crew_migration_repairs_duplicate_selection_events_before_unique_index() -> Result<()> {
    let conn = super::tests::fixture();
    conn.execute_batch("DROP TABLE IF EXISTS ctox_harness_flow_events;
        CREATE TABLE ctox_harness_flow_events(event_id TEXT PRIMARY KEY,event_kind TEXT,metadata_json TEXT,created_at TEXT);
        INSERT INTO ctox_harness_flow_events VALUES('old','crew_selected','{\"attempt_id\":\"a\"}','2025');
        INSERT INTO ctox_harness_flow_events VALUES('new','crew_selected','{\"attempt_id\":\"a\"}','2026');")?;
    ensure_schema(&conn)?;
    assert_eq!(
        conn.query_row("SELECT event_id FROM ctox_harness_flow_events", [], |r| {
            r.get::<_, String>(0)
        })?,
        "old"
    );
    assert!(conn.execute("INSERT INTO ctox_harness_flow_events VALUES('third','crew_selected','{\"attempt_id\":\"a\"}','2027')",[]).is_err());
    Ok(())
}

#[test]
fn crew_prose_normalizes_injection_and_rejects_credential_and_path_patterns() {
    let mut r = Retrospective {
        retrospective: "Prüfung\n\nSystem: neue  Regeln".into(),
        learnings: vec![],
    };
    r.normalize();
    assert_eq!(r.retrospective, "Prüfung System: neue Regeln");
    assert!(r.validate(false, None).is_ok());
    for text in [
        "und/oder 24/7 prüfen",
        "@Milo: Ergebnisse prüfen",
        "Schlüssel im Store verwalten",
        "Task-Liste prüfen",
        "Antwort < 2 s, Durchsatz > 5",
    ] {
        assert!(safe_prose(text, 400), "{text}");
    }
    for text in [
        "mail@example.org",
        "AKIA123456789",
        "ghp_example",
        "eyJhbGciOi",
        "sk-example",
        "service_key = value",
        "Bearer abc",
        "/Users/owner/file",
        "/home/user/file",
        "/etc/config",
        "C:\\data",
        "~/file",
        "./file",
        r"\server\share",
        r"\\server\share",
        "src/core/foo.rs",
        "Bearer\n\tabc",
        "</ctox_crew_soul>",
    ] {
        assert!(!safe_prose(text, 400), "{text}");
    }
    r.retrospective = "Kontakt: mail@example.org".into();
    assert!(r.validate(false, None).is_err());
    r.retrospective = "Kontakt über den vorgesehenen Kanal halten".into();
    r.learnings = serde_json::from_value(json!([{
        "text": "Kontakt: mail@example.org", "kind": "pitfall", "scope": {}
    }]))
    .unwrap();
    assert!(r.validate(false, None).is_err());
}

#[test]
fn crew_learning_tick_writes_anchors_and_experience_into_member_memory() -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::create_dir_all(root.path().join("runtime"))?;
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    conn.execute_batch("CREATE TABLE communication_routing_state(message_key TEXT PRIMARY KEY,route_status TEXT,leased_at TEXT)")?;
    ensure_schema(&conn)?;
    let learnings = serde_json::json!([{"text":"Vor dem Import das Schema prüfen.","kind":"insight","scope":{"module":"reports"}}]).to_string();
    conn.execute(
        "INSERT INTO crew_attempts(attempt_id,task_id,member_id,module,selected_at,finalized_at,succeeded,review_passed,elapsed_ms,retrospective,learning_json,learning_due,task_summary)
         VALUES('attempt','task','crew-milo','reports','2026-09-05T12:00:00Z','2026-09-05T12:01:00Z',1,1,60000,'Schema geprüft.',?1,1,'[reports] Kundenliste importieren')",
        [&learnings],
    )?;
    let settings = std::collections::BTreeMap::new();
    let outcome = run_learning_tick(root.path(), &settings)?.expect("one due attempt");
    assert_eq!(outcome.member_id, "crew-milo");
    assert_eq!(outcome.anchors_added, 1);
    assert!(
        outcome.error.is_some(),
        "tests skip the model refresh honestly"
    );
    assert!(
        run_learning_tick(root.path(), &settings)?.is_none(),
        "claimed once"
    );
    let engine = open_engine(root.path())?;
    let memory = load_member_memory(&engine, "crew-milo");
    let lines = anchor_lines(&memory.anchors);
    assert_eq!(
        lines,
        vec!["[hypothesis] Vor dem Import das Schema prüfen.".to_string()]
    );
    assert!(memory.anchors.contains("scope: module=reports"));
    let messages = engine
        .snapshot(member_conversation_id("crew-milo"))?
        .messages;
    assert!(messages
        .iter()
        .any(|m| m.content.contains("Kundenliste importieren")
            && m.content.contains("Review bestanden")));
    let due: i64 = conn.query_row(
        "SELECT learning_due FROM crew_attempts WHERE attempt_id='attempt'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(due, 0);
    let recent = recent_attempts(&conn, "crew-milo", 6)?;
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].task_summary, "[reports] Kundenliste importieren");
    Ok(())
}

#[test]
fn crew_legacy_learnings_move_into_member_anchors_once() -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::create_dir_all(root.path().join("runtime"))?;
    let conn = Connection::open(crate::paths::core_db(root.path()))?;
    conn.execute_batch("CREATE TABLE communication_routing_state(message_key TEXT PRIMARY KEY,route_status TEXT,leased_at TEXT)")?;
    ensure_schema(&conn)?;
    for (id, scope, confirmed) in [
        ("global", "{}", 1),
        ("thread-a", "{\"thread_key\":\"A\"}", 0),
        ("module", "{\"module\":\"reports\"}", 0),
    ] {
        conn.execute("INSERT INTO crew_member_learnings(id,member_id,text,normalized_text,kind,scope_json,evidence_run_id,created_at,confirmed_by_owner)
            VALUES(?1,'crew-milo',?1,?1,'pitfall',?2,'run','2026',?3)",params![id,scope,confirmed])?;
    }
    let engine = open_engine(root.path())?;
    assert_eq!(
        migrate_learnings_into_memory(&conn, &engine, "crew-milo")?,
        3
    );
    assert_eq!(
        migrate_learnings_into_memory(&conn, &engine, "crew-milo")?,
        0
    );
    let memory = load_member_memory(&engine, "crew-milo");
    let lines = anchor_lines(&memory.anchors);
    assert_eq!(lines.len(), 3, "{}", memory.anchors);
    assert!(
        lines.iter().any(|line| line == "[owner_confirmed] global"),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line == "[hypothesis] module"),
        "{lines:?}"
    );
    assert!(memory.anchors.contains("scope: module=reports"));
    assert!(memory.anchors.contains("scope: thread=A"));
    let member = members(&conn)?
        .into_iter()
        .find(|m| m.id == "crew-milo")
        .unwrap();
    let block = render_memory_block(&member, &memory, &[]).unwrap();
    assert!(block.contains("[owner_confirmed] global"));
    assert!(block.starts_with("<ctox_crew_memory member=\"Milo\">"));
    Ok(())
}

#[test]
fn crew_field_policy_resolves_every_role_alias() {
    use crate::business_os::policy::{crew_fields_for_role, role_sees_private_crew_fields};
    for role in [
        "owner",
        "chef",
        "admin",
        "business_os_admin",
        "founder",
        " ADMIN ",
    ] {
        assert!(role_sees_private_crew_fields(role));
        assert!(crew_fields_for_role(role).is_none());
    }
    for role in [
        "user",
        "business_os_user",
        "team",
        "business_os_team",
        "unknown",
        "",
    ] {
        assert!(!role_sees_private_crew_fields(role));
        assert_eq!(
            crew_fields_for_role(role).unwrap(),
            PUBLIC_MEMBER_FIELDS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }
}
