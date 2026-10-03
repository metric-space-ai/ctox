use super::*;

#[test]
fn adapter_reconciliation_intake_preserves_superseded_outcome() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let command = |id: &str| BusinessCommand {
        id: Some(id.into()),
        module: "outbound".into(),
        command_type: "outbound.research.adapters.reconcile".into(),
        record_id: Some("research-policy".into()),
        payload: serde_json::json!({"configuration_digest":"fixture-v1","prompt":"Reconcile fixture adapters"}),
        client_context: serde_json::json!({"source":"business-os"}),
        origin: CommandOrigin::TrustedLocal,
    };
    let first = record_command(root.path(), command("adapter-intake-first"))?;
    let original_task = first.task_id.expect("first task");
    let duplicate = record_command(root.path(), command("adapter-intake-duplicate"))?;
    assert_eq!(duplicate.status, "cancelled");
    let duplicate_task = duplicate
        .task_id
        .expect("duplicate task keeps its own identity");
    assert_ne!(duplicate_task, original_task);
    assert_eq!(
        channels::load_queue_task(root.path(), &duplicate_task)?
            .unwrap()
            .route_status,
        "cancelled"
    );
    let conn = open_store(root.path())?;
    let status: String = conn.query_row(
        "SELECT status FROM business_commands WHERE command_id='adapter-intake-duplicate'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(
        status, "cancelled",
        "compatibility intake must not overwrite the canonical cancellation"
    );
    let raw: String = conn.query_row(
        "SELECT payload_json FROM business_records WHERE collection='business_commands' AND record_id='adapter-intake-duplicate'",
        [], |r| r.get(0),
    )?;
    let projected: Value = serde_json::from_str(&raw)?;
    assert_eq!(projected["status"], "cancelled");
    assert_eq!(
        channels::business_command_projection(root.path(), "adapter-intake-duplicate")?["result"]
            ["superseded_by_task_id"],
        original_task
    );
    assert_eq!(
        record_command(root.path(), command("adapter-intake-duplicate"))?.status,
        "cancelled"
    );
    Ok(())
}

#[test]
fn incident_queue_cancel_child() -> anyhow::Result<()> {
    let Ok(root) = std::env::var("CTOX_TEST_incident_CANCEL_ROOT") else {
        return Ok(());
    };
    // Fresh process: no open_store call has registered the projection hooks.
    let action =
        std::env::var("CTOX_TEST_incident_QUEUE_ACTION").unwrap_or_else(|_| "cancel".to_string());
    let note_flag = if action == "complete" {
        "--note"
    } else {
        "--reason"
    };
    handle_queue_cli(
        Path::new(&root),
        &[
            action,
            "--message-key".into(),
            std::env::var("CTOX_TEST_incident_CANCEL_KEY")?,
            note_flag.into(),
            "operator settled task".into(),
        ],
    )
}

#[test]
fn incident_queue_cancel_in_fresh_process_projects_cancelled_to_rxdb() -> anyhow::Result<()> {
    assert_queue_cli_projection_after_store_reopen("cancel", "cancelled", "cancelled")
}

#[test]
fn incident_queue_fail_in_fresh_process_projects_failed_after_store_reopen() -> anyhow::Result<()> {
    assert_queue_cli_projection_after_store_reopen("fail", "failed", "failed")
}

#[test]
fn incident_queue_unreviewed_complete_preserves_running_projection_after_store_reopen(
) -> anyhow::Result<()> {
    assert_queue_cli_projection_after_store_reopen("complete", "leased", "running")
}

fn assert_queue_cli_projection_after_store_reopen(
    action: &str,
    expected_route: &str,
    expected_status: &str,
) -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let root = root.path();
    let task = channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title: "incident stale lease".into(),
            prompt: "Research a lead".into(),
            thread_key: "incident/cancel-projection".into(),
            workspace_root: None,
            priority: "normal".into(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )?;
    channels::lease_queue_task(root, &task.message_key, "previous-worker")?;
    let payload = serde_json::json!({
        "id": task.message_key, "status": "running", "route_status": "leased",
        "task_status": "running", "updated_at_ms": 1, "_rev": "1-old",
        "lease_owner": "previous-worker", "leased_at": "2000-01-01T00:00:00Z"
    })
    .to_string();
    let conn = open_store(root)?;
    conn.execute("INSERT INTO business_records (collection, record_id, rev, deleted, updated_at_ms, payload_json) VALUES ('ctox_queue_tasks', ?1, '1-old', 0, 1, ?2)", params![task.message_key, payload])?;
    drop(conn);
    let rxdb = Connection::open(rxdb_store_path(root))?;
    rxdb.execute_batch("CREATE TABLE ctox_business_os__ctox_queue_tasks__v1 (id TEXT PRIMARY KEY NOT NULL, revision TEXT, deleted INTEGER NOT NULL DEFAULT 0, lastWriteTime REAL NOT NULL DEFAULT 0, data TEXT NOT NULL)")?;
    rxdb.execute("INSERT INTO ctox_business_os__ctox_queue_tasks__v1 (id, revision, data) VALUES (?1, '1-old', ?2)", params![task.message_key, payload])?;
    drop(rxdb);
    let child = std::process::Command::new(std::env::current_exe()?)
        .args(["incident_queue_cancel_child", "--nocapture"])
        .env("CTOX_TEST_incident_CANCEL_ROOT", root)
        .env("CTOX_TEST_incident_CANCEL_KEY", &task.message_key)
        .env("CTOX_TEST_incident_QUEUE_ACTION", action)
        .output()?;
    anyhow::ensure!(
        child.status.success() == (action != "complete"),
        "queue CLI child failed: {} {}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert_eq!(
        channels::load_queue_task(root, &task.message_key)?
            .unwrap()
            .route_status,
        expected_route
    );
    // The CLI process exited without opening the Business OS store. Opening
    // it afterwards must expose the same terminal state in its durable mirror.
    let conn = open_store(root)?;
    let mirror: String = conn.query_row(
        "SELECT json_extract(payload_json, '$.status') FROM business_records
         WHERE collection='ctox_queue_tasks' AND record_id=?1",
        params![task.message_key],
        |row| row.get(0),
    )?;
    assert_eq!(mirror, expected_status);
    drop(conn);
    let rxdb = Connection::open(rxdb_store_path(root))?;
    let (raw_json, revision): (String, String) = rxdb.query_row(
        "SELECT data, revision FROM ctox_business_os__ctox_queue_tasks__v1 WHERE id=?1",
        params![task.message_key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let document: Value = serde_json::from_str(&raw_json)?;
    assert_eq!(document["status"], expected_status);
    assert_eq!(document["route_status"], expected_route);
    assert_eq!(document["task_status"], expected_status);
    if action == "complete" {
        assert_eq!(document["lease_owner"], "previous-worker");
    } else {
        assert!(document["lease_owner"].is_null());
        assert!(document["leased_at"].is_null());
    }
    if action == "cancel" {
        assert!(document["acked_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }
    assert_eq!(document["_rev"], revision);
    assert_eq!(
        document["acked_at"].as_str(),
        channels::load_queue_task(root, &task.message_key)?
            .unwrap()
            .acked_at
            .as_deref()
    );
    if action == "complete" {
        assert_eq!(
            revision, "1-old",
            "rejected completion must not rewrite the projection"
        );
        let core = Connection::open(crate::paths::core_db(root))?;
        let accepted_completion: bool = core.query_row(
            "SELECT EXISTS(SELECT 1 FROM ctox_core_transition_proofs
             WHERE entity_id=?1 AND to_state='Completed' AND accepted=1)",
            params![task.message_key],
            |row| row.get(0),
        )?;
        // Rejection rolls back the entire queue transaction, including its
        // attempted proof insert. No accepted completion may survive it.
        assert!(!accepted_completion);
        assert!(String::from_utf8_lossy(&child.stderr).contains("ctox process-mining guidance"));
    } else {
        assert_ne!(revision, "1-old");
    }
    Ok(())
}
