// Origin: CTOX
// License: Apache-2.0

use super::*;

#[test]
fn incident_reconcile_seven_cancelled_routes_with_historical_running_projections(
) -> anyhow::Result<()> {
    let temp = fixture()?;
    let root = temp.path();
    let old = BUSINESS_OS_QUEUE_ORPHAN_REPAIR_AGE_MS + 60_000;
    let mut keys = Vec::new();
    for index in 0..7 {
        let task = task(root, &format!("historical-cancel-{index}"))?;
        let core = Connection::open(crate::paths::core_db(root))?;
        core.execute(
            "UPDATE communication_routing_state
             SET route_status='cancelled', acked_at='2000-01-01T00:00:00Z'
             WHERE message_key=?1",
            params![task.message_key],
        )?;
        drop(core);
        // A recently rewritten projection must not mask an older canonical cancel.
        seed(
            root,
            &task.message_key,
            if index < 4 { old } else { 0 },
            "running",
        )?;
        let rxdb = Connection::open(rxdb_store_path(root))?;
        rxdb.execute(
            "UPDATE ctox_business_os__ctox_queue_tasks__v1
             SET data=json_set(data, '$.route_status', 'leased', '$.task_status', 'running',
                              '$.lease_owner', 'ctox-service', '$.status_note', 'Review feedback applied')
             WHERE id=?1",
            params![task.message_key],
        )?;
        keys.push(task.message_key);
    }
    // Even a not-yet-cleared worker key cannot override durable cancellation.
    let active_keys = HashSet::from([keys[0].clone()]);
    assert_eq!(reconcile_stale_queue_projections(root, &active_keys)?, 7);
    let conn = open_store(root)?;
    for key in keys {
        let payload = native(root, &key)?;
        for field in ["status", "route_status", "task_status", "terminal_status"] {
            assert_eq!(payload[field], "cancelled", "{key}: {field}");
        }
        assert!(payload["lease_owner"].is_null());
        assert_eq!(payload["message_key"], key);
        let mirror: String = conn.query_row(
            "SELECT json_extract(payload_json, '$.status') FROM business_records
             WHERE collection='ctox_queue_tasks' AND record_id=?1",
            params![key],
            |row| row.get(0),
        )?;
        assert_eq!(mirror, "cancelled");
        let core = Connection::open(crate::paths::core_db(root))?;
        let unchanged: bool = core.query_row(
            "SELECT route_status='cancelled' AND acked_at='2000-01-01T00:00:00Z'
             FROM communication_routing_state WHERE message_key=?1",
            params![key],
            |row| row.get(0),
        )?;
        assert!(unchanged);
    }
    assert_eq!(reconcile_stale_queue_projections(root, &HashSet::new())?, 0);
    Ok(())
}

#[test]
fn incident_reconcile_expired_projection_protects_current_worker_keys() -> anyhow::Result<()> {
    let temp = fixture()?;
    let root = temp.path();
    let protected = task(root, "protected")?;
    let expired = task(root, "expired")?;
    for task in [&protected, &expired] {
        channels::lease_queue_task(root, &task.message_key, "ctox-service")?;
        seed(
            root,
            &task.message_key,
            BUSINESS_OS_QUEUE_ORPHAN_REPAIR_AGE_MS + 60_000,
            "running",
        )?;
    }
    let core = Connection::open(crate::paths::core_db(root))?;
    core.execute("UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE route_status='leased'", [])?;
    drop(core);
    let active = HashSet::from([protected.message_key.clone()]);
    assert_eq!(reconcile_stale_queue_projections(root, &active)?, 1);
    assert_eq!(native(root, &protected.message_key)?["status"], "running");
    let failed = native(root, &expired.message_key)?;
    assert_eq!(failed["status"], "failed");
    assert!(failed["error"]
        .as_str()
        .unwrap()
        .contains("no live canonical queue lease"));
    assert_eq!(
        channels::load_queue_task(root, &expired.message_key)?
            .unwrap()
            .route_status,
        "leased"
    );
    Ok(())
}

fn fixture() -> anyhow::Result<tempfile::TempDir> {
    let temp = tempfile::tempdir()?;
    drop(channels::open_channel_db(&crate::paths::core_db(
        temp.path(),
    ))?);
    drop(open_store(temp.path())?);
    let conn = Connection::open(rxdb_store_path(temp.path()))?;
    conn.execute_batch(
        "CREATE TABLE ctox_business_os__ctox_queue_tasks__v1 (
             id TEXT PRIMARY KEY NOT NULL, revision TEXT,
             deleted INTEGER NOT NULL DEFAULT 0,
             lastWriteTime REAL NOT NULL DEFAULT 0, data TEXT NOT NULL)",
    )?;
    Ok(temp)
}

fn seed(root: &Path, id: &str, age_ms: i64, status: &str) -> anyhow::Result<Value> {
    let updated = now_ms() as i64 - age_ms;
    let payload = serde_json::json!({
        "id": id, "title": "Legacy queue projection", "status": status,
        "updated_at_ms": updated, "_rev": "1-old", "_deleted": false,
        "_meta": {"lwt": updated}
    });
    let conn = Connection::open(rxdb_store_path(root))?;
    conn.execute(
        "INSERT INTO ctox_business_os__ctox_queue_tasks__v1
             (id, revision, lastWriteTime, data) VALUES (?1, '1-old', ?2, ?3)",
        params![id, updated, serde_json::to_string(&payload)?],
    )?;
    Ok(payload)
}

fn task(root: &Path, title: &str) -> anyhow::Result<channels::QueueTaskView> {
    channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title: title.to_string(),
            prompt: title.to_string(),
            thread_key: format!("reconcile/{title}"),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )
}

fn native(root: &Path, id: &str) -> anyhow::Result<Value> {
    let conn = Connection::open(rxdb_store_path(root))?;
    let (raw_json, revision, last_write) = conn.query_row(
        "SELECT data, revision, lastWriteTime FROM ctox_business_os__ctox_queue_tasks__v1 WHERE id=?1",
        params![id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, f64>(2)?)),
    )?;
    let payload: Value = serde_json::from_str(&raw_json)?;
    assert_eq!(payload["_rev"], revision);
    assert_eq!(payload["_meta"]["lwt"].as_f64(), Some(last_write));
    Ok(payload)
}

#[test]
fn incident_reconcile_native_only_orphans_preserves_live_leases_and_canonical_outcomes(
) -> anyhow::Result<()> {
    let temp = fixture()?;
    let root = temp.path();
    let old = BUSINESS_OS_QUEUE_ORPHAN_REPAIR_AGE_MS + 60_000;
    let live = task(root, "live")?;
    channels::lease_queue_task(root, &live.message_key, "ctox-service")?;
    let pending = task(root, "pending")?;
    let cancelled = task(root, "cancelled")?;
    let core = Connection::open(crate::paths::core_db(root))?;
    core.execute(
        "UPDATE communication_routing_state SET route_status='cancelled' WHERE message_key=?1",
        params![cancelled.message_key],
    )?;
    // A legacy alias is resolved through the persisted command/task link.
    core.execute(
        "INSERT INTO business_command_aggregates
        (command_id, idempotency_key, payload_hash, module, command_type,
         execution_mode, execution_phase, intent_json, created_at_ms, updated_at_ms)
        VALUES ('live-command', 'live-command', 'fixture-hash', 'ctox',
                'business_os.chat.task', 'queue', 'leased', '{}', 1, 1)",
        [],
    )?;
    core.execute("INSERT INTO business_command_task_links (command_id, task_id, created_at_ms) VALUES ('live-command', ?1, 1)", params![live.message_key])?;
    drop(core);
    let live_before = seed(root, &live.message_key, old, "running")?;
    assert!(live_before.get("message_key").is_none());
    seed(root, &pending.message_key, old, "running")?;
    seed(root, &cancelled.message_key, old, "running")?;
    seed(root, "live-alias", old, "running")?;
    let rxdb = Connection::open(rxdb_store_path(root))?;
    rxdb.execute("UPDATE ctox_business_os__ctox_queue_tasks__v1 SET data=json_set(data, '$.command_id', 'live-command') WHERE id='live-alias'", [])?;
    drop(rxdb);
    for index in 0..7 {
        seed(root, &format!("orphan-{index}"), old, "running")?;
    }
    let fresh = seed(root, "fresh-orphan", 0, "running")?;
    let terminal = seed(root, "terminal-projection", old, "completed")?;
    let conn = open_store(root)?;
    // A stale mirror must not resurrect a completed native projection.
    upsert_business_record(
        &conn,
        "ctox_queue_tasks",
        "terminal-projection",
        1,
        serde_json::json!({"id":"terminal-projection", "status":"running"}),
    )?;
    upsert_business_record(
        &conn,
        "ctox_queue_tasks",
        "mirror-only-orphan",
        1,
        serde_json::json!({"id":"mirror-only-orphan", "status":"running"}),
    )?;
    conn.execute("INSERT INTO business_commands (command_id, module, command_type, status, payload_json, client_context_json, observed_at_ms)
                  VALUES ('completed-command', 'ctox', 'business_os.chat.task', 'completed', '{}', '{}', 1)", [])?;
    drop(conn);
    seed(root, "completed-alias", old, "running")?;
    let rxdb = Connection::open(rxdb_store_path(root))?;
    rxdb.execute("UPDATE ctox_business_os__ctox_queue_tasks__v1 SET data=json_set(data, '$.command_id', 'completed-command') WHERE id='completed-alias'", [])?;
    drop(rxdb);

    assert_eq!(
        reconcile_stale_queue_projections(root, &HashSet::new())?,
        11
    );
    assert_eq!(native(root, &live.message_key)?, live_before);
    assert_eq!(native(root, "live-alias")?["status"], "running");
    assert_eq!(native(root, "fresh-orphan")?, fresh);
    assert_eq!(native(root, "terminal-projection")?, terminal);
    assert_eq!(native(root, &pending.message_key)?["status"], "queued");
    assert_eq!(native(root, &cancelled.message_key)?["status"], "cancelled");
    assert_eq!(native(root, "completed-alias")?["status"], "completed");
    let conn = open_store(root)?;
    for id in (0..7)
        .map(|index| format!("orphan-{index}"))
        .chain(["mirror-only-orphan".to_string()])
    {
        let payload = native(root, &id)?;
        assert_eq!(payload["status"], "failed");
        assert_eq!(payload["terminal_status"], "failed");
        assert!(payload["error"]
            .as_str()
            .unwrap()
            .contains("no canonical queue row"));
        assert_ne!(payload["_rev"], "1-old");
        let mirror: String = conn.query_row("SELECT json_extract(payload_json, '$.status') FROM business_records WHERE collection='ctox_queue_tasks' AND record_id=?1", params![id], |row| row.get(0))?;
        assert_eq!(mirror, "failed");
    }
    drop(conn);
    assert_eq!(
        channels::load_queue_task(root, &live.message_key)?
            .unwrap()
            .route_status,
        "leased"
    );
    assert_eq!(
        channels::load_queue_task(root, &pending.message_key)?
            .unwrap()
            .route_status,
        "pending"
    );
    assert_eq!(
        channels::load_queue_task(root, &cancelled.message_key)?
            .unwrap()
            .route_status,
        "cancelled"
    );
    assert_eq!(reconcile_stale_queue_projections(root, &HashSet::new())?, 0);
    Ok(())
}

#[test]
fn incident_reconcile_projection_rollback_keeps_both_stores_unchanged() -> anyhow::Result<()> {
    let temp = fixture()?;
    let root = temp.path();
    let before = seed(
        root,
        "rollback-orphan",
        BUSINESS_OS_QUEUE_ORPHAN_REPAIR_AGE_MS + 60_000,
        "running",
    )?;
    let conn = Connection::open(rxdb_store_path(root))?;
    conn.execute_batch("CREATE TRIGGER reject_projection_update BEFORE UPDATE ON ctox_business_os__ctox_queue_tasks__v1 BEGIN SELECT RAISE(ABORT, 'test projection write failure'); END;")?;
    drop(conn);
    assert!(reconcile_stale_queue_projections(root, &HashSet::new()).is_err());
    assert_eq!(native(root, "rollback-orphan")?, before);
    let conn = open_store(root)?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM business_records WHERE collection='ctox_queue_tasks' AND record_id='rollback-orphan'", [], |row| row.get(0))?;
    assert_eq!(
        count, 0,
        "failed native write must roll back its mirror insert"
    );
    Ok(())
}
