// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::business_os::workjet_supervisor_execution_contract as wire;
use crate::service::harness_flow::{record_harness_flow_event, RecordHarnessFlowEventRequest};
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
fn fixture() -> anyhow::Result<(TempDir, Value)> {
    let root = super::supervisor_turns::fixture()?;
    let value = super::supervisor_turns::control(
        root.path(),
        "submit",
        "owner",
        "submit",
        json!({"project_id":"project","thread_id":THREAD,"goal":"Inspect the actual native supervisor run"}),
    )?;
    assert_eq!(value["status"], "completed");
    Ok((root, value["result"]["turn"].clone()))
}
fn watch(root: &Path, turn: &Value, id: &str, page: Option<Value>) -> anyhow::Result<Value> {
    let mut payload =
        json!({"project_id":"project","thread_id":THREAD,"target_command_id":turn["command_id"]});
    if let Some(page) = page {
        payload["execution_page"] = page;
    }
    super::supervisor_turns::control(root, id, "owner", "watch", payload)
}
fn event(
    root: &Path,
    turn: &Value,
    attempt: &str,
    kind: &str,
    metadata: Value,
) -> anyhow::Result<String> {
    let mut metadata = metadata;
    metadata["attempt_id"] = json!(attempt);
    metadata["command_id"] = turn["command_id"].clone();
    Ok(record_harness_flow_event(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: kind,
            title: "Native event",
            body_text: "PRIVATE RAW REASONING",
            message_key: turn["task_id"].as_str(),
            work_id: None,
            ticket_key: None,
            attempt_index: Some(47),
            metadata,
        },
    )?
    .event_id)
}
fn page(value: &Value) -> &Value {
    assert_eq!(value["status"], "completed", "{value}");
    &value["result"]["execution_page"]
}
fn rejected(value: anyhow::Result<Value>) {
    assert!(
        value.is_err()
            || value
                .as_ref()
                .is_ok_and(|value| value["status"] == "failed"),
        "{value:?}"
    );
}

#[test]
fn legacy_watch_is_unchanged_and_pending_turn_has_no_invented_execution_identity(
) -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let old = watch(root.path(), &turn, "old", None)?;
    assert_eq!(
        old["result"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        ["ok", "contract", "binding", "turn", "status", "task_status"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    assert_eq!(old["result"]["status"], "completed");
    assert_eq!(old["result"]["task_status"], "completed");
    let value = watch(root.path(), &turn, "page", Some(json!({})))?;
    let observed = page(&value);
    assert_eq!(value["result"]["turn"], old["result"]["turn"]);
    assert_eq!(value["result"]["execution_contract"], wire::CONTRACT_SCHEMA);
    assert_eq!(observed["attempt"], Value::Null);
    assert_eq!(observed["events"], json!([]));
    assert_eq!(observed["has_more"], false);
    assert_eq!(observed["task_id"], turn["task_id"]);
    Ok(())
}

#[test]
fn persisted_attempt_and_cursor_page_keep_late_native_events_without_ordinal_ids(
) -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let first = event(
        root.path(),
        &turn,
        "worker-attempt:actual",
        "worker.turn_started",
        json!({}),
    )?;
    let second = event(
        root.path(),
        &turn,
        "worker-attempt:actual",
        "worker.tool_started",
        json!({"tool":{"name":"native.tool","call_id":"call-actual"}}),
    )?;
    let one = watch(root.path(), &turn, "one", Some(json!({"limit":1})))?;
    assert_eq!(page(&one)["attempt"]["attempt_id"], "worker-attempt:actual");
    assert_eq!(page(&one)["attempt"]["attempt_index"], 47);
    assert_eq!(page(&one)["attempt"]["run_id"], Value::Null);
    assert_eq!(page(&one)["events"][0]["id"], first);
    assert_eq!(page(&one)["has_more"], true);
    let two = watch(
        root.path(),
        &turn,
        "two",
        Some(
            json!({"limit":1,"attempt_id":"worker-attempt:actual","cursor":page(&one)["next_cursor"]}),
        ),
    )?;
    assert_eq!(page(&two)["events"][0]["id"], second);
    assert_eq!(page(&two)["has_more"], false);
    let late = event(
        root.path(),
        &turn,
        "worker-attempt:actual",
        "worker.tool_completed",
        json!({"tool":{"name":"native.tool","call_id":"call-actual","success":true}}),
    )?;
    Connection::open(crate::paths::core_db(root.path()))?.execute(
        "UPDATE ctox_harness_flow_events SET created_at='2020-01-01T00:00:00Z' WHERE event_id=?1",
        [&late],
    )?;
    let three = watch(
        root.path(),
        &turn,
        "late",
        Some(json!({"attempt_id":"worker-attempt:actual","cursor":page(&two)["next_cursor"]})),
    )?;
    assert_eq!(page(&three)["events"][0]["id"], late);
    assert_eq!(page(&three)["events"][0]["success"], true);
    assert!(
        page(&three)["events"][0]["sequence"].as_u64().unwrap()
            > page(&two)["events"][0]["sequence"].as_u64().unwrap()
    );
    Ok(())
}

#[test]
fn latest_and_selected_attempts_never_mix_events() -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let old = event(
        root.path(),
        &turn,
        "attempt-old",
        "worker.turn_started",
        json!({}),
    )?;
    let new = event(
        root.path(),
        &turn,
        "attempt-new",
        "worker.turn_started",
        json!({}),
    )?;
    let selected = watch(
        root.path(),
        &turn,
        "selected",
        Some(json!({"attempt_id":"attempt-old"})),
    )?;
    assert_eq!(page(&selected)["events"].as_array().unwrap().len(), 1);
    assert_eq!(page(&selected)["events"][0]["id"], old);
    let latest = watch(root.path(), &turn, "latest", Some(json!({})))?;
    assert_eq!(page(&latest)["attempt"]["attempt_id"], "attempt-new");
    assert_eq!(page(&latest)["events"][0]["id"], new);
    rejected(watch(
        root.path(),
        &turn,
        "unknown",
        Some(json!({"attempt_id":"foreign-attempt"})),
    ));
    rejected(watch(
        root.path(),
        &turn,
        "old-cursor-on-new",
        Some(json!({"cursor":page(&selected)["next_cursor"]})),
    ));
    Ok(())
}

#[test]
fn foreign_actor_project_or_task_cannot_observe_native_facts() -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    event(
        root.path(),
        &turn,
        "private-attempt",
        "worker.turn_started",
        json!({}),
    )?;
    for (id, actor, project, target) in [
        (
            "foreign",
            "foreign",
            "project",
            turn["command_id"].as_str().unwrap(),
        ),
        (
            "project",
            "owner",
            "foreign-project",
            turn["command_id"].as_str().unwrap(),
        ),
        ("command", "owner", "project", "foreign-command"),
    ] {
        rejected(super::supervisor_turns::control(
            root.path(),
            id,
            actor,
            "watch",
            json!({"project_id":project,"thread_id":THREAD,"target_command_id":target,"execution_page":{}}),
        ));
    }
    let conn = open_store(root.path())?;
    let mut project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    project["owner_user_id"] = json!("foreign");
    store::upsert_business_record(&conn, "workjet_projects", "project", 3, project)?;
    rejected(watch(
        root.path(),
        &turn,
        "ownership-changed",
        Some(json!({})),
    ));
    Ok(())
}

#[test]
fn cursor_requires_the_exact_native_anchor_and_a_retained_event() -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let id = event(
        root.path(),
        &turn,
        "attempt",
        "worker.turn_started",
        json!({}),
    )?;
    let first = watch(root.path(), &turn, "first", Some(json!({})))?;
    let mut bad = page(&first)["next_cursor"].clone();
    bad["after_event_id"] = json!("another-event");
    rejected(watch(
        root.path(),
        &turn,
        "forged",
        Some(json!({"attempt_id":"attempt","cursor":bad})),
    ));
    Connection::open(crate::paths::core_db(root.path()))?.execute(
        "DELETE FROM ctox_harness_flow_events WHERE event_id=?1",
        [id],
    )?;
    rejected(watch(
        root.path(),
        &turn,
        "expired",
        Some(json!({"attempt_id":"attempt","cursor":page(&first)["next_cursor"]})),
    ));
    for bad in [
        json!({"limit":0}),
        json!({"limit":51}),
        json!({"owner_user_id":"foreign"}),
        json!({"attempt_id":" "}),
        json!({"cursor":{"after_sequence":0,"after_event_id":"x"}}),
    ] {
        rejected(watch(
            root.path(),
            &turn,
            &uuid::Uuid::new_v4().to_string(),
            Some(bad),
        ));
    }
    Ok(())
}

#[test]
fn observer_returns_only_eligible_safe_event_fields_without_raw_reasoning_or_tool_arguments(
) -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let visible = event(
        root.path(),
        &turn,
        "attempt",
        "worker.tool_completed",
        json!({"private":"SECRET RAW METADATA","tool":{"name":"safe.tool","call_id":"safe-call","success":false,"arguments":{"secret":"SECRET ARGUMENT"}}}),
    )?;
    event(
        root.path(),
        &turn,
        "attempt",
        "worker.thinking",
        json!({"cockpit_eligible":false,"private":"SECRET INELIGIBLE"}),
    )?;
    let value = watch(root.path(), &turn, "safe", Some(json!({})))?;
    assert_eq!(page(&value)["events"].as_array().unwrap().len(), 1);
    assert_eq!(page(&value)["events"][0]["id"], visible);
    let text = serde_json::to_string(page(&value))?;
    assert!(!text.contains("SECRET"));
    assert!(!text.contains("PRIVATE RAW REASONING"));
    assert!(!text.contains("arguments"));
    Ok(())
}

#[test]
fn run_id_is_exposed_only_from_the_canonical_durable_finalization_record() -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    event(
        root.path(),
        &turn,
        "actual-finalization-id",
        "worker.turn_started",
        json!({}),
    )?;
    let db = crate::paths::core_db(root.path());
    let engine = crate::lcm::LcmEngine::open(&db, crate::lcm::LcmConfig::default())?;
    let saved =
        engine.begin_worker_attempt_finalization(crate::lcm::WorkerAttemptFinalizationInput {
            attempt_id: "actual-finalization-id",
            work_key: "observer-native-work",
            conversation_id: 7404,
            source_label: "queue",
            agent_outcome: crate::lcm::AgentOutcome::Success,
            reply_text: "Saved native result",
            error_text: None,
        })?;
    let value = watch(root.path(), &turn, "finalization", Some(json!({})))?;
    assert_eq!(page(&value)["attempt"]["run_id"], saved.attempt_id);
    assert_eq!(page(&value)["attempt"]["attempt_id"], saved.attempt_id);
    assert_eq!(page(&value)["attempt"]["status"], saved.status);
    Ok(())
}

#[test]
fn event_reader_uses_a_read_snapshot_while_a_core_writer_holds_an_uncommitted_transaction(
) -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let id = event(
        root.path(),
        &turn,
        "attempt",
        "worker.turn_started",
        json!({}),
    )?;
    let mut core = Connection::open(crate::paths::core_db(root.path()))?;
    let tx = core.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute(
        "UPDATE ctox_harness_flow_events SET title='UNCOMMITTED' WHERE event_id=?1",
        [&id],
    )?;
    let observed = crate::business_os::project_chats::supervisor_observation::page(
        root.path(),
        &turn,
        &wire::ExecutionPageRequest {
            attempt_id: None,
            cursor: None,
            limit: None,
        },
    )?;
    assert_eq!(observed.events.len(), 1);
    assert_eq!(observed.events[0].title, "Native event");
    tx.rollback()?;
    Ok(())
}

#[test]
fn admitted_active_run_id_survives_reader_reopen_and_terminal_finalization() -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let tasks = vec![turn["task_id"].as_str().unwrap().to_owned()];
    let db = crate::paths::core_db(root.path());
    let run_id = crate::lcm::run_register_worker_run(
        &db,
        crate::lcm::WorkerRunInput {
            attempt_id: "native-active-attempt",
            work_key: "native-work",
            conversation_id: 42,
            source_label: "queue",
            task_ids: &tasks,
        },
    )?;
    event(
        root.path(),
        &turn,
        "native-active-attempt",
        "worker.turn_started",
        json!({}),
    )?;
    let first = watch(root.path(), &turn, "active-first", Some(json!({})))?;
    assert_eq!(page(&first)["attempt"]["run_id"], run_id);
    assert_eq!(
        page(&first)["attempt"]["attempt_id"],
        "native-active-attempt"
    );
    assert!(page(&first)["attempt"]["status"].is_null());
    assert!(page(&first)["attempt"]["finished_at_ms"].is_null());
    let reopened = watch(root.path(), &turn, "active-reopened", Some(json!({})))?;
    assert_eq!(page(&reopened)["attempt"]["run_id"], run_id);
    let conn = Connection::open(&db)?;
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM worker_attempt_finalizations",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(count, 0);
    let engine = crate::lcm::LcmEngine::open(&db, crate::lcm::LcmConfig::default())?;
    engine.begin_worker_attempt_finalization(crate::lcm::WorkerAttemptFinalizationInput {
        attempt_id: "native-active-attempt",
        work_key: "native-work",
        conversation_id: 42,
        source_label: "queue",
        agent_outcome: crate::lcm::AgentOutcome::Success,
        reply_text: "actual result",
        error_text: None,
    })?;
    let terminal = watch(root.path(), &turn, "finalizing-same-run", Some(json!({})))?;
    assert_eq!(page(&terminal)["attempt"]["run_id"], run_id);
    assert_eq!(page(&terminal)["attempt"]["status"], "finalizing");
    Ok(())
}

#[test]
fn registered_native_run_rejects_a_different_task_even_with_an_attempt_flow_event(
) -> anyhow::Result<()> {
    let (root, turn) = fixture()?;
    let tasks = vec!["foreign-task".to_owned()];
    crate::lcm::run_register_worker_run(
        &crate::paths::core_db(root.path()),
        crate::lcm::WorkerRunInput {
            attempt_id: "foreign-bound-attempt",
            work_key: "foreign-work",
            conversation_id: 42,
            source_label: "queue",
            task_ids: &tasks,
        },
    )?;
    event(
        root.path(),
        &turn,
        "foreign-bound-attempt",
        "worker.turn_started",
        json!({}),
    )?;
    rejected(watch(root.path(), &turn, "wrong-run-task", Some(json!({}))));
    Ok(())
}
