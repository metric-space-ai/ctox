// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use serde_json::json;

const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
fn gateway(actor: &str) -> Value {
    json!({"auth_source":"ctox_dev_managed_mcp_token","channel":"ctox_dev_managed_mcp",
        "surface":"workjet","actor":actor,"role":"chef","workspace":"tenant:source-owner","instance_id":"source-instance"})
}
fn call(root: &Path, actor: &str, args: Value) -> anyhow::Result<Value> {
    super::super::call_tool_inner(root, TOOL, args, Some(&gateway(actor)))
}
fn register(root: &Path) -> anyhow::Result<Value> {
    call(
        root,
        "owner",
        json!({"action":"register_source","source_environment_id":"source-env",
        "source_supervisor_thread_id":THREAD,"project_id":"project"}),
    )
}
fn poll(root: &Path, actor: &str) -> anyhow::Result<Value> {
    call(
        root,
        actor,
        json!({"action":"poll","source_environment_id":"source-env"}),
    )
}
pub(super) fn fixture() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    store::tests::seed_business_user(root.path(), "owner", "chef")?;
    store::tests::seed_business_user(root.path(), "foreign", "chef")?;
    save_mcp_policy(root.path(), &default_mcp_policy())?;
    super::super::super::store_workjet_projects::tests::create_workjet_rxdb_projection_tables(
        root.path(),
    )?;
    let rxdb = Connection::open(store::rxdb_store_path(root.path()))?;
    let schemas: Value = serde_json::from_str(include_str!("business_os_schema_contract.json"))?;
    for collection in [
        "user_threads",
        "user_thread_messages",
        "user_thread_states",
        "user_notifications",
        "business_commands",
        "ctox_queue_tasks",
        "ctox_runs",
        "ctox_harness_events",
    ] {
        let version = schemas[collection]["version"]
            .as_u64()
            .context("fixture schema")?;
        rxdb.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS ctox_business_os__{collection}__v{version}
            (id TEXT PRIMARY KEY NOT NULL,revision TEXT,deleted INTEGER NOT NULL DEFAULT 0,
             lastWriteTime REAL NOT NULL DEFAULT 0,data TEXT NOT NULL);"
        ))?;
    }
    store::upsert_business_record(
        &store::open_store(root.path())?,
        "workjet_projects",
        "project",
        1,
        json!({"id":"project","name":"Project","owner_user_id":"owner","status":"active","is_deleted":false}),
    )?;
    let bound = crate::business_os::command_plane::accept_rxdb_business_command(
        root.path(),
        json!({
        "id":"bind-supervisor","module":"ctox","command_type":"ctox.workjet.project.supervisor.bind",
        "record_id":"project","payload":{"project_id":"project","thread_id":THREAD},
        "client_context":{"actor":{"id":"owner","role":"chef","is_admin":true}}}),
    )?;
    anyhow::ensure!(
        bound["status"] == "completed",
        "fixture native supervisor admission failed"
    );
    Ok(root)
}
pub(super) fn queued_supervisor(root: &Path) -> anyhow::Result<String> {
    queued_supervisor_named(root, "submit-supervisor")
}
fn queued_supervisor_named(root: &Path, submit_id: &str) -> anyhow::Result<String> {
    let accepted = crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({
        "id":submit_id,"module":"ctox","command_type":"ctox.workjet.project.supervisor.turn.submit",
        "record_id":"project","payload":{"project_id":"project","thread_id":THREAD,"goal":"Make a small tested change"},
        "client_context":{"actor":{"id":"owner","role":"chef","is_admin":true}}}),
    )?;
    anyhow::ensure!(
        accepted["status"] == "completed",
        "fixture supervisor turn admission failed"
    );
    let id = accepted
        .pointer("/result/turn/command_id")
        .and_then(Value::as_str)
        .context("fixture command")?
        .to_owned();
    let queued = crate::channels::load_queue_task_for_business_os_command(root, &id)?
        .context("fixture queue link")?;
    let core = Connection::open(crate::paths::core_db(root))?;
    let changed = core.execute("UPDATE communication_routing_state SET route_status='leased',lease_owner='fixture-service',
        lease_worker_id='fixture-worker-1',leased_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
        lease_expires_at=strftime('%Y-%m-%dT%H:%M:%fZ','now','+30 minutes') WHERE message_key=?1",
        [&queued.message_key])?;
    anyhow::ensure!(changed == 1, "fixture actual routing lease missing");
    Ok(id)
}
pub(super) fn session(root: &Path) -> anyhow::Result<(String, Value)> {
    session_named(root, "submit-supervisor")
}
fn session_named(root: &Path, submit_id: &str) -> anyhow::Result<(String, Value)> {
    let id = queued_supervisor_named(root, submit_id)?;
    let command = crate::channels::business_command_projection(root, &id)?;
    let token = issue_internal_command_session_token(
        root,
        &id,
        command["payload_hash"].as_str().context("hash")?,
        "owner",
        "chef",
        "native-job-workspace",
        &json!({}),
    )?;
    let token = restrict_internal_command_session_to_workjet_supervisor(root, &token)?;
    let trusted = verify_internal_command_session_token(root, &token)?;
    Ok((token, trusted))
}
fn dispatch(root: &Path, trusted: &Value) -> anyhow::Result<Value> {
    super::super::call_tool_inner(
        root,
        TOOL,
        json!({"action":"dispatch","dispatch_key":"worker-one",
        "task":"Make a small tested change","computer_id":"native-computer"}),
        Some(trusted),
    )
}
fn complete(registration: &Value, intent: &Value, result: Value) -> Value {
    json!({"action":"complete","registration_id":registration["registrationId"],"revision":registration["revision"],
        "intent_id":intent["intentId"],"result":result})
}
fn success(intent: &Value) -> Value {
    json!({"schemaVersion":1,"status":"dispatched","environmentId":"target-env",
        "workerThreadId":intent["intentId"],"computerId":"native-computer","branch":"codex/worker",
        "worktreePath":"/private/worktrees/worker","parent":{"environmentId":"source-env","threadId":THREAD},
        "modelSelection":{"instanceId":"source-env","model":"claude-opus-5-5","options":[{"id":"reasoning","value":"high"}]},
        "enabledCapabilityIds":["repository_read","run_checks"]})
}

#[test]
fn workjet_dispatch_actual_mcp_signed_supervisor_queue_and_lost_acks() -> anyhow::Result<()> {
    let root = fixture()?;
    let mut read_only = default_mcp_policy();
    read_only.allow_writes = false;
    save_mcp_policy(root.path(), &read_only)?;
    assert!(register(root.path()).is_err());
    save_mcp_policy(root.path(), &default_mcp_policy())?;
    let registration = register(root.path())?;
    assert_eq!(register(root.path())?, registration);
    let (token, trusted) = session(root.path())?;
    assert!(trusted["workjet_supervisor_only"] == true);
    assert!(crew_only_session_allows_tool(TOOL, Some(&trusted)));
    assert!(!crew_only_session_allows_tool(
        "business_os.execute_action",
        Some(&trusted)
    ));
    // Reject a title that Workjet cannot decode before it poisons the poll head.
    assert!(super::super::call_tool_inner(
        root.path(),
        TOOL,
        json!({"action":"dispatch","dispatch_key":"oversized-title","task":"Make a change",
            "title":"x".repeat(201)}),
        Some(&trusted)
    )
    .is_err());
    let first = dispatch(root.path(), &trusted)?;
    let intent = &first["intent"];
    uuid::Uuid::parse_str(intent["intentId"].as_str().context("intent UUID")?)?;
    assert_eq!(dispatch(root.path(), &trusted)?, first);
    assert_eq!(poll(root.path(), "owner")?["intents"], json!([intent]));
    assert_eq!(poll(root.path(), "owner")?["intents"], json!([intent]));
    assert!(poll(root.path(), "foreign")?["intents"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(super::super::call_tool_inner(
        root.path(),
        TOOL,
        json!({"action":"dispatch","dispatch_key":"worker-one","task":"Changed"}),
        Some(&trusted)
    )
    .is_err());
    let result = success(intent);
    let args = complete(&registration, intent, result.clone());
    let ack = call(root.path(), "owner", args.clone())?;
    assert_eq!(ack["intentId"], intent["intentId"]);
    assert_eq!(call(root.path(), "owner", args)?, ack);
    assert!(poll(root.path(), "owner")?["intents"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut changed = result;
    changed["branch"] = json!("codex/different");
    assert!(call(
        root.path(),
        "owner",
        complete(&registration, intent, changed)
    )
    .is_err());
    // The source records one existing dispatcher result. Native creates no worker.
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    assert_eq!(
        core.query_row(
            "SELECT count(*) FROM workjet_worker_dispatch_intents",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    assert!(verify_internal_command_session_token(root.path(), &token).is_ok());
    Ok(())
}

#[test]
fn workjet_dispatch_source_isolation_tombstone_and_revision_fence() -> anyhow::Result<()> {
    let root = fixture()?;
    let registration = register(root.path())?;
    let (_, trusted) = session(root.path())?;
    let intent = dispatch(root.path(), &trusted)?["intent"].clone();
    assert!(call(
        root.path(),
        "foreign",
        complete(&registration, &intent, success(&intent))
    )
    .is_err());
    let mut forged = gateway("owner");
    forged["workspace"] = json!("foreign-instance");
    assert!(super::super::call_tool_inner(
        root.path(),
        TOOL,
        complete(&registration, &intent, success(&intent)),
        Some(&forged)
    )
    .is_err());
    let revoke = json!({"action":"revoke_source","registration_id":registration["registrationId"],"revision":registration["revision"]});
    let revoked = call(root.path(), "owner", revoke.clone())?;
    assert_eq!(call(root.path(), "owner", revoke)?, revoked);
    assert!(
        register(root.path()).is_err(),
        "polling/register replay cannot revive a tombstone"
    );
    assert!(dispatch(root.path(), &trusted).is_err());
    assert!(poll(root.path(), "owner")?["intents"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(call(
        root.path(),
        "owner",
        complete(&registration, &intent, success(&intent))
    )
    .is_err());
    let explicit = call(
        root.path(),
        "owner",
        json!({"action":"register_source","source_environment_id":"source-env",
        "source_supervisor_thread_id":THREAD,"project_id":"project","expected_revision":revoked["revision"]}),
    )?;
    assert!(explicit["revision"].as_u64() > revoked["revision"].as_u64());
    assert!(
        poll(root.path(), "owner")?["intents"]
            .as_array()
            .unwrap()
            .is_empty(),
        "old revision cannot reappear"
    );
    assert!(
        dispatch(root.path(), &trusted).is_err(),
        "old dispatch_key cannot change registration"
    );
    Ok(())
}

#[test]
fn workjet_dispatch_revalidates_exact_execution_lease_and_native_authority() -> anyhow::Result<()> {
    for mode in [
        "replaced-worker",
        "expired-lease",
        "cancelled",
        "epoch",
        "owner",
        "project-retired",
        "supervisor-retired",
    ] {
        let root = fixture()?;
        register(root.path())?;
        let (_, trusted) = session(root.path())?;
        let core = Connection::open(crate::paths::core_db(root.path()))?;
        let policy = store::open_store(root.path())?;
        match mode {
            "replaced-worker" => {
                core.execute("UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE route_status='leased'",[])?;
            }
            "expired-lease" => {
                core.execute("UPDATE communication_routing_state SET lease_expires_at='2000-01-01' WHERE route_status='leased'",[])?;
            }
            "cancelled" => {
                core.execute("UPDATE communication_routing_state SET route_status='cancelled' WHERE route_status='leased'",[])?;
            }
            "epoch" => {
                policy.execute("UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id='owner'",[])?;
            }
            "owner" => {
                policy.execute(
                    "UPDATE business_users SET active=0 WHERE user_id='owner'",
                    [],
                )?;
            }
            "project-retired" | "supervisor-retired" => {
                let (collection, id) = if mode == "project-retired" {
                    ("workjet_projects", "project")
                } else {
                    ("user_threads", THREAD)
                };
                let mut record = store::outbound_load_record(&policy, collection, id)?.unwrap();
                record["is_deleted"] = json!(true);
                store::upsert_business_record(&policy, collection, id, now_ms(), record)?;
            }
            _ => unreachable!(),
        }
        assert!(dispatch(root.path(), &trusted).is_err(), "{mode}");
        assert_eq!(
            core.query_row(
                "SELECT count(*) FROM workjet_worker_dispatch_intents",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
    }
    Ok(())
}

#[test]
fn workjet_dispatch_rejects_forged_context_non_supervisor_and_unbounded_results(
) -> anyhow::Result<()> {
    let root = fixture()?;
    register(root.path())?;
    assert!(super::super::call_tool_inner(
        root.path(),
        TOOL,
        json!({"action":"dispatch","dispatch_key":"x","task":"x",
        "_context":{"actor":"owner","role":"chef","workjet_supervisor_only":true}}),
        None
    )
    .is_err());
    assert!(call(
        root.path(),
        "owner",
        json!({"action":"dispatch","dispatch_key":"x","task":"x"})
    )
    .is_err());
    let (_, trusted) = session(root.path())?;
    assert!(super::super::call_tool_inner(
        root.path(),
        TOOL,
        json!({"action":"poll","source_environment_id":"source-env"}),
        Some(&trusted)
    )
    .is_err());
    let first = dispatch(root.path(), &trusted)?;
    let intent: Intent = serde_json::from_value(first["intent"].clone())?;
    let valid = success(&first["intent"]);
    validate_result(&intent, &valid)?;
    for (pointer, value) in [
        ("/workerThreadId", json!("another-worker")),
        ("/parent/threadId", json!("foreign")),
        (
            "/modelSelection/options",
            json!({"secret":"test-placeholder"}),
        ),
        ("/modelSelection/options/0/value", json!(17)),
        ("/enabledCapabilityIds", json!(["same", "same"])),
        ("/extra", json!("test-placeholder")),
    ] {
        let mut bad = valid.clone();
        if pointer == "/extra" {
            bad["extra"] = value;
        } else {
            *bad.pointer_mut(pointer).context("fixture pointer")? = value;
        }
        assert!(validate_result(&intent, &bad).is_err(), "{pointer}");
    }
    assert!(validate_result(
        &intent,
        &json!({"schemaVersion":1,"status":"failed","reason":"remote-dispatch-pending"})
    )
    .is_err());
    validate_result(
        &intent,
        &json!({"schemaVersion":1,"status":"failed","reason":"remote-dispatch-failed"}),
    )?;
    Ok(())
}

fn observe_call(root: &Path, trusted: &Value, limit: Option<u32>) -> anyhow::Result<Value> {
    let mut request = json!({"action":"observe"});
    if let Some(limit) = limit {
        request["limit"] = json!(limit);
    }
    super::super::call_tool_inner(root, TOOL, request, Some(trusted))
}

#[test]
fn supervisor_observes_retained_start_receipts_in_a_later_real_turn() -> anyhow::Result<()> {
    let root = fixture()?;
    let registration = register(root.path())?;
    let (_, trusted) = session(root.path())?;
    assert_eq!(
        observe_call(root.path(), &trusted, None)?["observations"],
        json!([])
    );
    let first = dispatch(root.path(), &trusted)?;
    let pending = observe_call(root.path(), &trusted, None)?;
    assert_eq!(pending["projectId"], "project");
    assert_eq!(pending["supervisorThreadId"], THREAD);
    assert_eq!(
        pending["observations"][0]["intentId"],
        first["intent"]["intentId"]
    );
    assert_eq!(pending["observations"][0]["dispatchKey"], "worker-one");
    assert_eq!(pending["observations"][0]["registrationCurrent"], true);
    assert!(pending["observations"][0]["acknowledgement"].is_null());
    assert!(pending["observations"][0]["execution"].is_null());
    call(
        root.path(),
        "owner",
        complete(&registration, &first["intent"], success(&first["intent"])),
    )?;
    let (_, next) = session_named(root.path(), "submit-supervisor-next")?;
    let observed = observe_call(root.path(), &next, None)?;
    let ack = &observed["observations"][0]["acknowledgement"];
    assert_eq!(ack["status"], "dispatched");
    assert_eq!(ack["workerThreadId"], first["intent"]["intentId"]);
    assert_eq!(ack["computerId"], "native-computer");
    assert!(ack.get("modelSelection").is_none());
    assert!(observed["observations"][0]["execution"].is_null());
    assert_eq!(observed["truncated"], false);
    assert_eq!(poll(root.path(), "owner")?["intents"], json!([]));
    Ok(())
}

#[test]
fn supervisor_observation_is_bounded_read_only_without_schema_repair() -> anyhow::Result<()> {
    let root = fixture()?;
    let (_, trusted) = session(root.path())?;
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    let absent: bool = core.query_row("SELECT NOT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_worker_dispatch_intents')", [], |row| row.get(0))?;
    assert!(absent);
    let mut read_only = default_mcp_policy();
    read_only.allow_writes = false;
    save_mcp_policy(root.path(), &read_only)?;
    assert_eq!(
        observe_call(root.path(), &trusted, None)?["observations"],
        json!([])
    );
    assert!(core.query_row("SELECT NOT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_worker_dispatch_intents')", [], |row| row.get::<_,bool>(0))?);
    assert!(register(root.path()).is_err());
    save_mcp_policy(root.path(), &default_mcp_policy())?;
    let registration = register(root.path())?;
    for key in ["first", "second", "third"] {
        let dispatched = super::super::call_tool_inner(
            root.path(),
            TOOL,
            json!({"action":"dispatch","dispatch_key":key,"task":"A real scoped task"}),
            Some(&trusted),
        )?;
        call(
            root.path(),
            "owner",
            complete(
                &registration,
                &dispatched["intent"],
                json!({"schemaVersion":1,"status":"failed","reason":"computer-unavailable"}),
            ),
        )?;
    }
    core.execute_batch("BEGIN IMMEDIATE")?;
    let observed = observe_call(root.path(), &trusted, Some(2))?;
    core.execute_batch("ROLLBACK")?;
    assert_eq!(observed["observations"].as_array().unwrap().len(), 2);
    assert_eq!(observed["observations"][0]["dispatchKey"], "third");
    assert_eq!(
        observed["observations"][0]["acknowledgement"]["reason"],
        "computer-unavailable"
    );
    assert_eq!(observed["truncated"], true);
    assert!(serde_json::to_vec(&observed)?.len() < 64 * 1024);
    assert!(observe_call(root.path(), &trusted, Some(0)).is_err());
    assert!(observe_call(root.path(), &trusted, Some(33)).is_err());
    assert!(super::super::call_tool_inner(
        root.path(),
        TOOL,
        json!({"action":"observe","project_id":"foreign"}),
        Some(&trusted)
    )
    .is_err());
    assert!(call(root.path(), "owner", json!({"action":"observe"})).is_err());
    Ok(())
}

#[test]
fn supervisor_observation_retains_stale_registration_without_reopening_it() -> anyhow::Result<()> {
    let root = fixture()?;
    let registration = register(root.path())?;
    let (_, trusted) = session(root.path())?;
    dispatch(root.path(), &trusted)?;
    call(
        root.path(),
        "owner",
        json!({"action":"revoke_source","registration_id":registration["registrationId"],"revision":registration["revision"]}),
    )?;
    let observed = observe_call(root.path(), &trusted, None)?;
    assert_eq!(observed["observations"][0]["registrationCurrent"], false);
    assert!(observed["observations"][0]["acknowledgement"].is_null());
    assert!(dispatch(root.path(), &trusted).is_err());
    Ok(())
}

#[test]
fn supervisor_observation_rejects_old_leases_and_foreign_owner_rows() -> anyhow::Result<()> {
    let root = fixture()?;
    register(root.path())?;
    let (_, trusted) = session(root.path())?;
    dispatch(root.path(), &trusted)?;
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    core.execute(
        "UPDATE workjet_worker_dispatch_sources SET project_id='another-project'",
        [],
    )?;
    assert_eq!(
        observe_call(root.path(), &trusted, None)?["observations"],
        json!([])
    );
    core.execute(
        "UPDATE workjet_worker_dispatch_sources SET project_id='project'",
        [],
    )?;
    core.execute(
        "UPDATE workjet_worker_dispatch_sources SET owner_user_id='foreign'",
        [],
    )?;
    assert_eq!(
        observe_call(root.path(), &trusted, None)?["observations"],
        json!([])
    );
    core.execute("UPDATE communication_routing_state SET lease_worker_id='new-worker' WHERE route_status='leased'", [])?;
    assert!(observe_call(root.path(), &trusted, None).is_err());
    Ok(())
}

fn outcome_receipt(intent: &Value) -> Value {
    json!({"schema":"ctox.workjet.worker-outcome.v1","worker_thread_id":intent["intentId"],
        "environment_id":"target-env","computer_id":"native-computer",
        "branch":format!("workjet/worker/{}",intent["intentId"].as_str().unwrap()),
        "execution_stopped":true,"pull_request":{"provider":"github","number":7,
            "url":"https://github.com/metric-space-ai/example/pull/7","head_oid":"a".repeat(40),"state":"merged"}})
}
fn report_outcome(registration: &Value, intent: &Value, receipt: Value) -> Value {
    json!({"action":"report_outcome","registration_id":registration["registrationId"],
        "revision":registration["revision"],"intent_id":intent["intentId"],"receipt":receipt})
}
fn isolated_startup(root: &Path, registration: &Value, intent: &Value) -> anyhow::Result<()> {
    let mut result = success(intent);
    result["branch"] = json!(format!(
        "workjet/worker/{}",
        intent["intentId"].as_str().unwrap()
    ));
    call(root, "owner", complete(registration, intent, result))?;
    Ok(())
}
fn future_meeting(root: &Path) -> anyhow::Result<()> {
    let corpus: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"
    ))?;
    let mut meeting = corpus["valid_cases"][0]["value"].clone();
    meeting["project_id"] = json!("project");
    meeting["owner_user_id"] = json!("owner");
    meeting["supervisor"] = json!({"workjet_thread_id":THREAD,"ctox_thread_key":format!("business-os/threads/{THREAD}")});
    meeting["state"] = json!("planned");
    meeting["revision"] = json!(0);
    meeting["deck_revision"] = json!(0);
    meeting["slides"] = json!([]);
    meeting["comments"] = json!([]);
    meeting["transcript"] = json!([]);
    meeting["todos"] = Value::Null;
    meeting["previous_goal"] = Value::Null;
    meeting["scheduled_at_ms"] = json!(now_ms() + 3_600_000);
    meeting["prepare_at_ms"] = json!(now_ms());
    let policy = store::open_store(root)?;
    policy.execute_batch(
        "CREATE TABLE workjet_jour_fixe_meetings (
        meeting_id TEXT PRIMARY KEY,project_id TEXT NOT NULL,owner_user_id TEXT NOT NULL,
        scheduled_at_ms INTEGER NOT NULL,metadata_json TEXT NOT NULL,preparation_task_id TEXT);",
    )?;
    policy.execute(
        "INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-1','project','owner',?1,?2,NULL)",
        params![
            meeting["scheduled_at_ms"].as_i64().unwrap(),
            meeting.to_string()
        ],
    )?;
    Ok(())
}

#[test]
fn worker_terminal_outcome_replays_after_reopen_and_reaches_next_deck_without_fake_execution(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let registration = register(root.path())?;
    let (_, trusted) = session(root.path())?;
    let dispatched = dispatch(root.path(), &trusted)?;
    let intent = &dispatched["intent"];
    let report = report_outcome(&registration, intent, outcome_receipt(intent));
    assert!(
        call(root.path(), "owner", report.clone()).is_err(),
        "startup is required"
    );
    isolated_startup(root.path(), &registration, intent)?;
    let retained = call(root.path(), "owner", report.clone())?;
    assert_eq!(retained["provenance"], "authenticated_source_report");
    assert_eq!(
        call(root.path(), "owner", report.clone())?,
        retained,
        "lost ACK replays identical persisted report"
    );
    let observed = observe_call(root.path(), &trusted, None)?;
    assert_eq!(observed["observations"][0]["reportedOutcome"], retained);
    assert!(observed["observations"][0]["execution"].is_null());
    let mut contradictory = report.clone();
    contradictory["receipt"]["pull_request"]["state"] = json!("closed");
    assert!(call(root.path(), "owner", contradictory).is_err());
    let mut different_pr = report;
    different_pr["receipt"]["pull_request"]["number"] = json!(8);
    different_pr["receipt"]["pull_request"]["url"] =
        json!("https://github.com/metric-space-ai/example/pull/8");
    assert!(call(root.path(), "owner", different_pr).is_err());
    future_meeting(root.path())?;
    // The real restricted MCP meeting reader opens fresh read-only connections.
    let deck = super::super::call_tool_inner(
        root.path(),
        workjet_jour_fixe::READ_TOOL,
        json!({"action":"read_meeting","request":{"project_id":"project","meeting_id":"meeting-1"}}),
        Some(&trusted),
    )?;
    assert_eq!(
        deck["worker_outcomes"]["reports"][0]["reported_outcome"],
        retained
    );
    assert_eq!(
        deck["worker_outcomes"]["reports"][0]["execution_key"],
        trusted["command_id"]
    );
    assert_eq!(deck["worker_outcomes"]["truncated"], false);
    assert_eq!(
        deck["meeting"]["state"], "planned",
        "worker report does not complete or narrate a meeting"
    );
    assert_eq!(
        deck["previous_goal_definition"],
        Value::Null,
        "worker report does not fabricate a confirmed goal"
    );
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    assert_eq!(
        core.query_row(
            "SELECT count(*) FROM workjet_worker_dispatch_outcomes",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

#[test]
fn worker_terminal_outcome_requires_current_source_and_exact_stopped_worker() -> anyhow::Result<()>
{
    for field in [
        "worker_thread_id",
        "environment_id",
        "computer_id",
        "branch",
        "execution_stopped",
        "head",
        "url",
    ] {
        let root = fixture()?;
        let registration = register(root.path())?;
        let (_, trusted) = session(root.path())?;
        let dispatched = dispatch(root.path(), &trusted)?;
        let intent = &dispatched["intent"];
        isolated_startup(root.path(), &registration, intent)?;
        let mut receipt = outcome_receipt(intent);
        match field {
            "execution_stopped" => receipt[field] = json!(false),
            "head" => receipt["pull_request"]["head_oid"] = json!("not-a-git-head"),
            "url" => {
                receipt["pull_request"]["url"] =
                    json!("https://github.com/metric-space-ai/example/pull/8")
            }
            _ => receipt[field] = json!("foreign"),
        }
        assert!(
            call(
                root.path(),
                "owner",
                report_outcome(&registration, intent, receipt)
            )
            .is_err(),
            "{field}"
        );
        assert!(read_outcome(
            &Connection::open(crate::paths::core_db(root.path()))?,
            "owner",
            "project",
            THREAD,
            intent["intentId"].as_str().unwrap()
        )?
        .is_null());
    }
    let root = fixture()?;
    let registration = register(root.path())?;
    let (_, trusted) = session(root.path())?;
    let dispatched = dispatch(root.path(), &trusted)?;
    let intent = &dispatched["intent"];
    isolated_startup(root.path(), &registration, intent)?;
    let report = report_outcome(&registration, intent, outcome_receipt(intent));
    assert!(call(root.path(), "foreign", report.clone()).is_err());
    assert!(
        super::super::call_tool_inner(root.path(), TOOL, report.clone(), Some(&trusted)).is_err(),
        "a model's restricted native session cannot forge a source report"
    );
    call(
        root.path(),
        "owner",
        json!({"action":"revoke_source","registration_id":registration["registrationId"],
        "revision":registration["revision"]}),
    )?;
    assert!(call(root.path(), "owner", report).is_err());
    Ok(())
}
