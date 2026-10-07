// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";

fn fixture() -> anyhow::Result<TempDir> {
    let root = project_fixture()?;
    let rxdb = Connection::open(store::rxdb_store_path(root.path()))?;
    let schemas: Value = serde_json::from_str(include_str!("../business_os_schema_contract.json"))?;
    for collection in ["user_thread_states", "user_notifications"] {
        let version = schemas[collection]["version"].as_u64().context("canonical Threads version")?;
        rxdb.execute_batch(&format!("CREATE TABLE IF NOT EXISTS ctox_business_os__{collection}__v{version} (
            id TEXT PRIMARY KEY NOT NULL, revision TEXT, deleted INTEGER NOT NULL DEFAULT 0,
            lastWriteTime REAL NOT NULL DEFAULT 0, data TEXT NOT NULL);"))?;
    }
    let bound = control(root.path(), "bind", "owner", "bind", json!({"project_id":"project","thread_id":THREAD}))?;
    ensure!(bound["result"]["binding"]["thread_id"] == THREAD, "native binding fixture failed");
    Ok(root)
}

fn control(root: &Path, operation: &str, actor: &str, action: &str, payload: Value) -> anyhow::Result<Value> {
    let kind = if action == "bind" { "ctox.workjet.project.supervisor.bind".to_owned() }
        else { format!("ctox.workjet.project.supervisor.turn.{action}") };
    crate::business_os::command_plane::accept_rxdb_business_command(root, json!({
        "id":operation,"module":"ctox","command_type":kind,"record_id":payload["project_id"],
        "payload":payload,"client_context":{"actor":{"id":actor,"role":"admin","is_admin":true}}
    }))
}
fn submit(root: &Path) -> anyhow::Result<Value> {
    control(root,"submit","owner","submit",json!({"project_id":"project","thread_id":THREAD,"goal":"Prepare the project report"}))
}
fn observe(command_id: &str) -> Value {
    json!({"project_id":"project","thread_id":THREAD,"target_command_id":command_id})
}
fn rejected(value: anyhow::Result<Value>) -> bool {
    value.is_err() || value.as_ref().is_ok_and(|v| v["ok"] == false || v["status"] == "failed")
}

#[test]
fn supervisor_submit_replays_one_native_turn_and_one_message_in_the_existing_uuid() -> anyhow::Result<()> {
    let root = fixture()?;
    let first = submit(root.path())?;
    let turn = &first["result"]["turn"];
    assert_eq!(first["status"], "completed");
    assert_eq!(first["result"]["contract"], "ctox.workjet.supervisor_turn.v1");
    assert_eq!(turn["thread_id"], THREAD);
    assert_eq!(turn["thread_key"], format!("business-os/threads/{THREAD}"));
    let target = turn["command_id"].as_str().context("native turn id")?;
    let queued = crate::mission::channels::load_queue_task_for_business_os_command(root.path(),target)?.context("native queue")?;
    assert_eq!(queued.thread_key, format!("business-os/threads/{THREAD}"));
    assert!(queued.prompt.contains("Prepare the project report"));
    let replay = submit(root.path())?;
    assert_eq!(replay["result"], first["result"]);
    assert_eq!(count(root.path(), THREADS)?, 1);
    assert_eq!(count(root.path(), "user_messages")?, 1);
    let changed = control(root.path(),"submit","owner","submit",json!({"project_id":"project","thread_id":THREAD,"goal":"Different intent"}));
    assert!(rejected(changed));
    assert_eq!(count(root.path(), "user_messages")?, 1);
    Ok(())
}

#[test]
fn supervisor_watch_uses_the_native_owner_and_queue_link_without_foreign_disclosure() -> anyhow::Result<()> {
    let root = fixture()?;
    let submitted = submit(root.path())?;
    let target = submitted["result"]["turn"]["command_id"].as_str().context("native turn id")?;
    let watched = control(root.path(),"watch","owner","watch",observe(target))?;
    assert_eq!(watched["result"]["turn"]["command_id"],target);
    assert_eq!(watched["result"]["turn"]["task_id"],submitted["result"]["turn"]["task_id"]);
    assert_eq!(watched["result"]["turn"]["terminal"],false);
    assert!(rejected(control(root.path(),"foreign-watch","foreign","watch",observe(target))));
    assert!(rejected(control(root.path(),"unknown-watch","owner","watch",observe("missing"))));
    let mut foreign_thread = observe(target);
    foreign_thread["thread_id"] = json!("b8958e94-4c45-4b47-b44b-03b9d8c47123");
    assert!(rejected(control(root.path(),"wrong-thread","owner","watch",foreign_thread)));
    assert_eq!(count(root.path(), "user_messages")?, 1);
    Ok(())
}

#[test]
fn supervisor_cancel_uses_the_existing_native_receipt_and_preserves_terminal_history() -> anyhow::Result<()> {
    let root = fixture()?;
    let submitted = submit(root.path())?;
    let target = submitted["result"]["turn"]["command_id"].as_str().context("native turn id")?;
    let mut request = observe(target); request["reason"] = json!("Owner cancelled this request");
    assert!(rejected(control(root.path(),"foreign-cancel","foreign","cancel",request.clone())));
    let cancelled = control(root.path(),"cancel","owner","cancel",request.clone())?;
    assert_eq!(cancelled["status"], "completed");
    assert_eq!(cancelled["result"]["turn"]["status"], "cancelled");
    assert_eq!(cancelled["result"]["cancellation"]["worker_interrupt_acknowledged"], false);
    let cancellation = cancelled["result"]["cancellation"]["command_id"].as_str().context("native cancellation id")?;
    let proof = crate::mission::channels::business_command_projection(root.path(), cancellation)?;
    assert_eq!(proof["status"], "completed");
    assert_eq!(proof["result"]["target_command_id"], target);
    assert_eq!(proof["result"]["execution_task_id"], submitted["result"]["turn"]["task_id"]);
    let replay = control(root.path(),"cancel","owner","cancel",request.clone())?;
    assert_eq!(replay["result"],cancelled["result"]);
    assert!(rejected(control(root.path(),"cancel-terminal-again","owner","cancel",request)));
    assert_eq!(crate::mission::channels::business_command_projection(root.path(),target)?["status"],"cancelled");
    assert_eq!(count(root.path(), "user_messages")?,1);
    Ok(())
}

#[test]
fn supervisor_controls_reject_forged_routes_external_executors_and_large_goals() -> anyhow::Result<()> {
    for (field,value) in [
        ("thread_key",json!("foreign-route")), ("owner_user_id",json!("foreign")),
        ("external_executor",json!({"computer_id":"foreign"})), ("risk_class",json!("external")),
        ("goal",json!("x".repeat(4097))), ("goal",json!(" ")),
    ] {
        let root = fixture()?;
        let mut request = json!({"project_id":"project","thread_id":THREAD,"goal":"Prepare report"});
        request[field] = value;
        assert!(rejected(control(root.path(),"forged","owner","submit",request)));
        assert_eq!(count(root.path(), "user_messages")?,0);
    }
    Ok(())
}

#[test]
fn supervisor_watch_refuses_a_native_turn_from_another_owned_thread() -> anyhow::Result<()> {
    let root = fixture()?;
    let conn = open_store(root.path())?;
    let mut other_project = outbound_load_record(&conn,"workjet_projects","project")?.unwrap();
    other_project["id"] = json!("other-project");
    store::upsert_business_record(&conn,"workjet_projects","other-project",2,other_project)?;
    let other_thread = "b8958e94-4c45-4b47-b44b-03b9d8c47123";
    control(root.path(),"other-bind","owner","bind",json!({"project_id":"other-project","thread_id":other_thread}))?;
    let other = control(root.path(),"other-submit","owner","submit",json!({"project_id":"other-project","thread_id":other_thread,"goal":"Other report"}))?;
    let target = other["result"]["turn"]["command_id"].as_str().context("other native turn id")?;
    assert!(rejected(control(root.path(),"cross-project-watch","owner","watch",observe(target))));
    assert_eq!(count(root.path(),"user_messages")?,1);
    Ok(())
}

#[test]
fn archived_project_cannot_submit_a_new_supervisor_turn() -> anyhow::Result<()> {
    let root = fixture()?;
    let conn = open_store(root.path())?;
    let mut project = outbound_load_record(&conn,"workjet_projects","project")?.unwrap();
    project["status"] = json!("archived");
    store::upsert_business_record(&conn,"workjet_projects","project",2,project)?;
    assert!(rejected(submit(root.path())));
    assert_eq!(count(root.path(),"user_messages")?,0);
    Ok(())
}
