// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
const OTHER: &str = "b8958e94-4c45-4b47-b44b-03b9d8c47123";

fn bind(root: &Path, operation: &str, actor: &str, payload: Value) -> anyhow::Result<Value> {
    crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({
            "id": operation, "module": "ctox", "command_type": "ctox.workjet.project.supervisor.bind",
            "payload": payload,
            "client_context": {"actor": {"id": actor, "role": "admin", "is_admin": true}}
        }),
    )
}

#[test]
fn supervisor_binding_preserves_existing_code_uuid_and_replays_without_history_reset(
) -> anyhow::Result<()> {
    let root = project_fixture()?;
    let payload = json!({"project_id":"project", "thread_id":THREAD});
    let first = bind(root.path(), "bind-supervisor", "owner", payload.clone())?;
    let binding = &first["result"]["binding"];
    assert_eq!(binding["thread_id"], THREAD);
    assert_eq!(
        binding["thread_key"],
        format!("business-os/threads/{THREAD}")
    );
    assert_eq!(binding["project_id"], "project");
    let conn = open_store(root.path())?;
    assert!(crate::business_os::domain_effect::contains(
        &conn,
        "bind-supervisor"
    )?);
    let original = outbound_load_record(&conn, THREADS, THREAD)?.unwrap();
    assert_eq!(original["metadata"]["workjet_supervisor"], *binding);
    assert_eq!(
        store::load_rxdb_collection_record(root.path(), THREADS, THREAD)?.unwrap()["metadata"],
        original["metadata"]
    );
    let mut later = original;
    later["title"] = json!("Renamed by the owner");
    later["last_message_id"] = json!("existing-message");
    later["last_message_at_ms"] = json!(91);
    store::upsert_business_record(&conn, THREADS, THREAD, 90, later.clone())?;
    // Preserve the full committed record, including its native revision and timestamps.
    let later = outbound_load_record(&conn, THREADS, THREAD)?.unwrap();
    let replay = bind(root.path(), "bind-supervisor", "owner", payload.clone())?;
    assert_eq!(replay["result"], first["result"]);
    let repeat = bind(root.path(), "bind-supervisor-again", "owner", payload)?;
    assert_eq!(repeat["result"], first["result"]);
    assert_eq!(
        outbound_load_record(&conn, THREADS, THREAD)?.unwrap(),
        later
    );
    assert_eq!(count(root.path(), THREADS)?, 1);
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM workjet_supervisor_bindings",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

#[test]
fn supervisor_binding_rejects_foreign_archived_forged_and_non_uuid_requests() -> anyhow::Result<()>
{
    for (actor, payload, archived) in [
        (
            "foreign",
            json!({"project_id":"project", "thread_id":THREAD}),
            false,
        ),
        (
            "owner",
            json!({"project_id":"missing", "thread_id":THREAD}),
            false,
        ),
        (
            "owner",
            json!({"project_id":"project", "thread_id":"made-up-session"}),
            false,
        ),
        (
            "owner",
            json!({"project_id":"project", "thread_id":"00000000-0000-0000-0000-000000000000"}),
            false,
        ),
        (
            "owner",
            json!({"project_id":"project", "thread_id":THREAD, "owner_user_id":"foreign"}),
            false,
        ),
        (
            "owner",
            json!({"project_id":"project", "thread_id":THREAD, "thread_key":"foreign-route"}),
            false,
        ),
        (
            "owner",
            json!({"project_id":"project", "thread_id":THREAD}),
            true,
        ),
    ] {
        let root = project_fixture()?;
        let conn = open_store(root.path())?;
        if archived {
            let mut project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
            project["status"] = json!("archived");
            store::upsert_business_record(&conn, "workjet_projects", "project", 2, project)?;
        }
        let result = bind(root.path(), "rejected-supervisor", actor, payload);
        assert!(
            result.is_err()
                || result.as_ref().unwrap()["ok"] == false
                || result.as_ref().unwrap()["status"] == "failed",
            "{result:?}"
        );
        assert_eq!(count(root.path(), THREADS)?, 0);
        assert!(!crate::business_os::domain_effect::contains(
            &conn,
            "rejected-supervisor"
        )?);
    }
    Ok(())
}

#[test]
fn supervisor_binding_refuses_adopting_or_replacing_history_and_cross_project_uuid(
) -> anyhow::Result<()> {
    let root = project_fixture()?;
    let conn = open_store(root.path())?;
    let history = json!({"id":THREAD,"owner_user_id":"owner", "title":"Unrelated history", "updated_at_ms":1});
    store::upsert_business_record(&conn, THREADS, THREAD, 1, history.clone())?;
    let history = outbound_load_record(&conn, THREADS, THREAD)?.unwrap();
    assert!(handle_command(
        root.path(),
        &command(
            "ctox.workjet.project.supervisor.bind",
            "adopt",
            json!({"project_id":"project","thread_id":THREAD})
        ),
        "owner"
    )
    .is_err());
    assert_eq!(
        outbound_load_record(&conn, THREADS, THREAD)?.unwrap(),
        history
    );
    handle_command(
        root.path(),
        &command(
            "ctox.workjet.project.supervisor.bind",
            "first",
            json!({"project_id":"project","thread_id":OTHER}),
        ),
        "owner",
    )?;
    assert!(handle_command(
        root.path(),
        &command(
            "ctox.workjet.project.supervisor.bind",
            "replace",
            json!({"project_id":"project","thread_id":THREAD})
        ),
        "owner"
    )
    .is_err());
    let mut second = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    second["id"] = json!("second");
    store::upsert_business_record(&conn, "workjet_projects", "second", 2, second)?;
    assert!(handle_command(
        root.path(),
        &command(
            "ctox.workjet.project.supervisor.bind",
            "cross-project",
            json!({"project_id":"second","thread_id":OTHER})
        ),
        "owner"
    )
    .is_err());
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM workjet_supervisor_bindings",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    assert!(!crate::business_os::domain_effect::contains(
        &conn, "adopt"
    )?);
    assert!(!crate::business_os::domain_effect::contains(
        &conn, "replace"
    )?);
    assert!(!crate::business_os::domain_effect::contains(
        &conn,
        "cross-project"
    )?);
    Ok(())
}

#[test]
fn competing_supervisor_bindings_leave_exactly_one_committed_identity() -> anyhow::Result<()> {
    let root = project_fixture()?;
    let barrier = Arc::new(Barrier::new(3));
    let workers = [("first", THREAD), ("second", OTHER)].map(|(operation, thread_id)| {
        let path = root.path().to_owned();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            handle_command(
                &path,
                &command(
                    "ctox.workjet.project.supervisor.bind",
                    operation,
                    json!({"project_id":"project","thread_id":thread_id}),
                ),
                "owner",
            )
        })
    });
    barrier.wait();
    let [a, b] = workers;
    let outcomes = [a.join().unwrap(), b.join().unwrap()];
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(count(root.path(), THREADS)?, 1);
    let conn = open_store(root.path())?;
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM workjet_supervisor_bindings",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

#[test]
fn registered_supervisor_uses_the_existing_native_ai_producer_and_retains_binding(
) -> anyhow::Result<()> {
    let root = project_fixture()?;
    let rxdb = Connection::open(store::rxdb_store_path(root.path()))?;
    let schemas: Value = serde_json::from_str(include_str!("../business_os_schema_contract.json"))?;
    for collection in ["user_thread_states", "user_notifications"] {
        let version = schemas[collection]["version"]
            .as_u64()
            .context("canonical Threads version")?;
        rxdb.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS ctox_business_os__{collection}__v{version} (
            id TEXT PRIMARY KEY NOT NULL, revision TEXT, deleted INTEGER NOT NULL DEFAULT 0,
            lastWriteTime REAL NOT NULL DEFAULT 0, data TEXT NOT NULL);"
        ))?;
    }
    let payload = json!({"project_id":"project", "thread_id":THREAD});
    let bound = bind(root.path(), "native-supervisor", "owner", payload.clone())?;
    let answer = crate::business_os::command_plane::accept_rxdb_business_command(
        root.path(),
        json!({
            "id":"supervisor-ai-request", "module":"threads", "command_type":"threads.ai.request",
            "payload":{"thread_id":THREAD,"goal":"Prepare the project report","risk_class":"internal"},
            "client_context":{"actor":{"id":"owner","role":"admin","is_admin":true}}
        }),
    )?;
    let ai_id = answer["result"]["ai_command"]["id"]
        .as_str()
        .or_else(|| answer["result"]["ai_command"]["command_id"].as_str())
        .context("native AI producer must return its accepted command")?;
    let queued =
        crate::mission::channels::load_queue_task_for_business_os_command(root.path(), ai_id)?
            .context("native AI command must have a durable queue task")?;
    assert_eq!(queued.thread_key, format!("business-os/threads/{THREAD}"));
    assert!(queued.prompt.contains("Prepare the project report"));
    let thread = outbound_load_record(&open_store(root.path())?, THREADS, THREAD)?.unwrap();
    assert_eq!(
        thread["metadata"]["workjet_supervisor"],
        bound["result"]["binding"]
    );
    assert!(!thread["last_message_id"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
    let repeated = bind(root.path(), "native-supervisor-repeat", "owner", payload)?;
    assert_eq!(repeated["result"], bound["result"]);
    assert_eq!(count(root.path(), THREADS)?, 1);
    Ok(())
}
