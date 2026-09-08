use super::*;

#[test]
fn chat_candidate_cursor_is_bounded_and_uses_native_indexes() -> Result<()> {
    let db = Connection::open_in_memory()?;
    db.execute_batch(
        "CREATE TABLE business_commands(command_id TEXT PRIMARY KEY, command_type TEXT,
            status TEXT, observed_at_ms INTEGER);
         CREATE TABLE cockpit_chat_delivery(command_id TEXT PRIMARY KEY, terminal INTEGER);",
    )?;
    ensure_chat_candidate_indexes(&db)?;
    for n in 0..100 {
        let id = format!("chat-{n:03}");
        db.execute(
            "INSERT INTO business_commands VALUES(?1,'business_os.chat.task','queued',?2)",
            params![id, n],
        )?;
        if n < 20 {
            db.execute("INSERT INTO cockpit_chat_delivery VALUES(?1,1)", [&id])?;
        }
    }
    let candidates = |cursor: &str| -> Result<Vec<String>> {
        Ok(db
            .prepare(CHAT_CANDIDATES_SQL)?
            .query_map([cursor], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    };
    let first = candidates("")?;
    assert_eq!(
        first.len(),
        33,
        "32 candidates plus one continuation sentinel"
    );
    assert_eq!(first[0], "chat-020", "settled deliveries are excluded");
    let next = candidates(&first[31])?;
    assert_eq!(
        next[0], "chat-052",
        "cursor resumes after the last processed candidate"
    );
    assert!(
        candidates("chat-099")?.is_empty(),
        "end of scan must not self-reschedule forever"
    );
    let plan = db
        .prepare(&format!("EXPLAIN QUERY PLAN {CHAT_CANDIDATES_SQL}"))?
        .query_map(["chat-051"], |r| r.get::<_, String>(3))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .join("\n");
    assert!(
        plan.contains("idx_cockpit_chat_candidates") && plan.contains("command_id>?"),
        "{plan}"
    );
    assert!(plan.contains("idx_cockpit_chat_terminal"), "{plan}");
    assert!(!plan.contains("USE TEMP B-TREE"), "{plan}");
    Ok(())
}

#[test]
fn projection_does_not_read_worker_messages_or_initialize_lcm() -> Result<()> {
    let root = tempfile::tempdir()?;
    let accepted = store::accept_rxdb_business_command(
        root.path(),
        json!({
            "id":"chat-no-history", "module":"research", "command_type":"business_os.chat.task",
            "payload":{"prompt":"Read the fixture"}, "client_context":{"source":"business-os-chat"}
        }),
    )?;
    let task_id = accepted["task_id"].as_str().expect("task");
    let rxdb = store_projections::tests::create_repair_rxdb_tables(root.path())?;
    rxdb.execute_batch("CREATE TABLE ctox_business_os__business_chats__v0(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER DEFAULT 0,lastWriteTime REAL DEFAULT 0,data TEXT NOT NULL)")?;
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    core.execute(
        "UPDATE communication_routing_state SET attempt=1 WHERE message_key=?1",
        [task_id],
    )?;
    // Replace transcript sources by views that fail if read. A large history
    // must have exactly the same cost/behavior as an absent history.
    core.execute_batch(
        "ALTER TABLE communication_messages RENAME TO hidden_communication_messages;
        CREATE VIEW communication_messages AS SELECT * FROM missing_transcript_source;
        CREATE VIEW messages AS SELECT * FROM missing_lcm_source;",
    )?;
    project(root.path(), &core)?;
    assert!(!core.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='task_execution_plan_revisions')", [], |row| row.get::<_, bool>(0))?, "projection must not initialize worker history");
    let business = store::open_store(root.path())?;
    assert_eq!(
        business.query_row(
            "SELECT COUNT(*) FROM cockpit_chat_delivery WHERE command_id='chat-no-history'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    project(root.path(), &core)?;
    Ok(())
}

#[test]
fn retry_wait_delivers_interim_without_closing_gates_or_replaying_trimmed_status() -> Result<()> {
    for (language, expected_status) in [
        ("de", "Der Versuch wird wiederholt"),
        ("en", "The attempt will be retried"),
    ] {
        let root = tempfile::tempdir()?;
        let accepted = store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id":"cockpit-chat","command_id":"cockpit-chat","module":"research",
                "command_type":"business_os.chat.task", "record_id":"research",
                "payload":{"prompt":"Read the fixture", "language":language},
                "client_context":{"source":"business-os-chat","module":"research"}
            }),
        )?;
        let task_id = accepted["task_id"].as_str().expect("queue task");
        let rxdb = store_projections::tests::create_repair_rxdb_tables(root.path())?;
        rxdb.execute_batch("CREATE TABLE ctox_business_os__business_chats__v0(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER DEFAULT 0,lastWriteTime REAL DEFAULT 0,data TEXT NOT NULL);")?;
        channels::lease_queue_task(root.path(), task_id, "fixture-worker")?;
        for phase in ["leased", "running"] {
            channels::transition_business_command_for_task(
                root.path(),
                task_id,
                phase,
                None,
                None,
                None,
                "fixture worker lifecycle",
            )?;
        }
        channels::persist_business_command_worker_result(
            root.path(),
            task_id,
            "A partial result is available.",
        )?;
        crate::service::harness_flow::record_harness_flow_event(
            root.path(),
            crate::service::harness_flow::RecordHarnessFlowEventRequest {
                event_kind: "worker.turn_started",
                title: "Fixture",
                body_text: "",
                message_key: Some(task_id),
                work_id: None,
                ticket_key: None,
                attempt_index: Some(1),
                metadata: json!({"attempt_id":"fixture-attempt","command_id":"cockpit-chat"}),
            },
        )?;
        let core = Connection::open(crate::paths::core_db(root.path()))?;
        core.execute("UPDATE communication_routing_state SET route_status='pending',hold_reason='Transient fixture',retry_not_before='2099-01-01T00:00:00Z' WHERE message_key=?1",[task_id])?;
        core.execute("UPDATE business_command_aggregates SET execution_phase='retry_wait' WHERE command_id='cockpit-chat'",[])?;
        project(root.path(), &core)?;
        // Plan evidence belongs to worker initialization, never to projection.
        let _lcm = crate::lcm::LcmEngine::open(
            &crate::paths::core_db(root.path()),
            crate::lcm::LcmConfig::default(),
        )?;
        for revision in [1, 2] {
            core.execute("INSERT INTO task_execution_plan_revisions(work_key,revision,task_id,command_id,attempt_id,plan_signature,steps_json,phase,completed_steps,total_steps,percent,review_status,created_at_ms,updated_at_ms) VALUES('fixture-work',?1,?2,'cockpit-chat','fixture-attempt',?3,?4,'working',0,1,0,'pending',?1,?1)",params![revision,task_id,format!("revision-{revision}"),json!([{"label":format!("Fixture step {revision}"),"status":"in_progress"}]).to_string()])?;
        }
        project(root.path(), &core)?;
        let business = store::open_store(root.path())?;
        let command = store::load_business_command(&business, "cockpit-chat")?;
        let chat_id = store_projections::business_chat_id(&command, "cockpit-chat");
        let raw:String=business.query_row("SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id=?1",[&chat_id],|r|r.get(0))?;
        let mut chat: Value = serde_json::from_str(&raw)?;
        let messages = chat["messages"].as_array().unwrap();
        for revision in [1, 2] {
            assert!(
                messages.iter().any(|m| m["kind"] == "status"
                    && m["text"]
                        .as_str()
                        .is_some_and(|text| text.contains(&format!("Fixture step {revision}")))),
                "coalesced delivery must replay both plan revisions"
            );
        }
        assert!(
            messages
                .iter()
                .any(|m| m["text"] == text(language, "leased")),
            "lease notification survives a fast transition to retry_wait"
        );
        assert!(messages.iter().any(|m| m["kind"] == "status"
            && m["text"]
                .as_str()
                .is_some_and(|s| s.starts_with(expected_status))));
        let interim = messages
            .iter()
            .find(|m| m["kind"] == "interim")
            .expect("interim before review completion");
        assert_eq!(interim["text"], "A partial result is available.");
        assert_eq!(interim["task_id"], task_id);
        assert_eq!(interim["command_id"], "cockpit-chat");
        assert_eq!(interim["run_id"], "fixture-attempt");
        assert!(!messages.iter().any(|m| m["kind"] == "reply"));
        assert_eq!(core.query_row("SELECT execution_phase FROM business_command_aggregates WHERE command_id='cockpit-chat'",[],|r|r.get::<_,String>(0))?,"retry_wait");
        chat["messages"]
            .as_array_mut()
            .unwrap()
            .retain(|m| m["kind"] != "status");
        store::BusinessProjectionWriter::open(root.path())?.upsert_source_projection(
            "business_chats",
            &chat_id,
            chrono::Utc::now().timestamp_millis(),
            chat,
        )?;
        project(root.path(), &core)?;
        let raw:String=business.query_row("SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id=?1",[&chat_id],|r|r.get(0))?;
        let replay: Value = serde_json::from_str(&raw)?;
        assert!(!replay["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["kind"] == "status"));
        core.execute("UPDATE communication_routing_state SET hold_reason='New retry cause' WHERE message_key=?1",[task_id])?;
        project(root.path(), &core)?;
        let raw:String=business.query_row("SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id=?1",[&chat_id],|r|r.get(0))?;
        let replay: Value = serde_json::from_str(&raw)?;
        let statuses = replay["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["kind"] == "status")
            .collect::<Vec<_>>();
        assert_eq!(
            statuses.len(),
            1,
            "changed state must not resurrect trimmed plan/lease messages"
        );
        assert!(statuses[0]["text"]
            .as_str()
            .unwrap()
            .contains("New retry cause"));
    }
    Ok(())
}

#[test]
fn terminal_chats_are_delivered_once_and_skipped_afterwards() -> Result<()> {
    let root = tempfile::tempdir()?;
    let accepted = store::accept_rxdb_business_command(
        root.path(),
        json!({
            "id":"cockpit-chat-terminal","command_id":"cockpit-chat-terminal","module":"research",
            "command_type":"business_os.chat.task", "record_id":"research",
            "payload":{"prompt":"Read the fixture", "language":"de"},
            "client_context":{"source":"business-os-chat","module":"research"}
        }),
    )?;
    let task_id = accepted["task_id"].as_str().expect("queue task");
    let rxdb = store_projections::tests::create_repair_rxdb_tables(root.path())?;
    rxdb.execute_batch("CREATE TABLE ctox_business_os__business_chats__v0(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER DEFAULT 0,lastWriteTime REAL DEFAULT 0,data TEXT NOT NULL);")?;
    channels::lease_queue_task(root.path(), task_id, "fixture-worker")?;
    for phase in ["leased", "running"] {
        channels::transition_business_command_for_task(
            root.path(),
            task_id,
            phase,
            None,
            None,
            None,
            "fixture worker lifecycle",
        )?;
    }
    crate::service::harness_flow::record_harness_flow_event(
        root.path(),
        crate::service::harness_flow::RecordHarnessFlowEventRequest {
            event_kind: "worker.turn_started",
            title: "Fixture",
            body_text: "",
            message_key: Some(task_id),
            work_id: None,
            ticket_key: None,
            attempt_index: Some(1),
            metadata: json!({"attempt_id":"fixture-attempt","command_id":"cockpit-chat-terminal"}),
        },
    )?;
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    let business = store::open_store(root.path())?;
    // Active task: delivered, not settled.
    project(root.path(), &core)?;
    let terminal_flag = |business: &Connection| -> Result<Option<i64>> {
        Ok(business
            .query_row(
                "SELECT terminal FROM cockpit_chat_delivery WHERE command_id='cockpit-chat-terminal'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()?)
    };
    assert_eq!(terminal_flag(&business)?, Some(0));
    // The task ends: the next pass delivers the final state and settles the chat.
    core.execute(
        "UPDATE communication_routing_state SET route_status='handled' WHERE message_key=?1",
        [task_id],
    )?;
    project(root.path(), &core)?;
    project(root.path(), &core)?;
    assert_eq!(terminal_flag(&business)?, Some(1));
    // A settled chat is skipped before any projection work: a later state change
    // that would add a status message for an active task leaves it untouched.
    let command = store::load_business_command(&business, "cockpit-chat-terminal")?;
    let chat_id = store_projections::business_chat_id(&command, "cockpit-chat-terminal");
    let before:String=business.query_row("SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id=?1",[&chat_id],|r|r.get(0))?;
    core.execute(
        "UPDATE communication_routing_state SET route_status='pending',hold_reason='Would add a message' WHERE message_key=?1",
        [task_id],
    )?;
    project(root.path(), &core)?;
    let after:String=business.query_row("SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id=?1",[&chat_id],|r|r.get(0))?;
    assert_eq!(before, after, "settled chats must not be re-projected");
    Ok(())
}
