use super::*;

#[test]
fn incident_queue_cancel_child() -> anyhow::Result<()> {
    let Ok(root) = std::env::var("CTOX_TEST_incident_CANCEL_ROOT") else {
        return Ok(());
    };
    // Fresh process: no open_store call has registered the projection hooks.
    handle_queue_cli(
        Path::new(&root),
        &[
            "cancel".into(),
            "--message-key".into(),
            std::env::var("CTOX_TEST_incident_CANCEL_KEY")?,
            "--reason".into(),
            "operator cancelled orphan".into(),
        ],
    )
}

#[test]
fn incident_queue_cancel_in_fresh_process_projects_cancelled_to_rxdb() -> anyhow::Result<()> {
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
        "task_status": "running", "updated_at_ms": 1
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
        .output()?;
    anyhow::ensure!(
        child.status.success(),
        "queue CLI child failed: {} {}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert_eq!(
        channels::load_queue_task(root, &task.message_key)?
            .unwrap()
            .route_status,
        "cancelled"
    );
    let rxdb = Connection::open(rxdb_store_path(root))?;
    let (raw, revision): (String, String) = rxdb.query_row(
        "SELECT data, revision FROM ctox_business_os__ctox_queue_tasks__v1 WHERE id=?1",
        params![task.message_key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let document: Value = serde_json::from_str(&raw)?;
    assert_eq!(document["status"], "cancelled");
    assert_eq!(document["route_status"], "cancelled");
    assert_eq!(document["_rev"], revision);
    assert_ne!(revision, "1-old");
    Ok(())
}
