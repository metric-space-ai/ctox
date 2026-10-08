// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::mission::{plan, schedule};

fn fixture() -> anyhow::Result<TempDir> {
    let root = super::jour_fixe_owner::fixture("review")?;
    let _ = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner",
        "Project owner",
        "admin",
        store::now_ms() as i64,
    )?;
    let mut meeting = super::jour_fixe_owner::saved(root.path())?;
    meeting["previous_goal"] = Value::Null;
    meeting["todos"] = json!({"meeting_id":"meeting-1","revision":1,"status":"proposed","items":[
        {"id":"prove-reopen","title":"Prove reopening","acceptance":"The saved answer survives reopening",
         "priority":"P1","owner":"Michael","due_at_ms":1791800000000i64,"evidence_ids":[]}]});
    save(root.path(), &meeting)?;
    Ok(root)
}
fn save(root: &Path, meeting: &Value) -> anyhow::Result<()> {
    let conn = open_store(root)?;
    conn.execute(
        "UPDATE workjet_jour_fixe_meetings SET metadata_json=?1 WHERE meeting_id='meeting-1'",
        [meeting.to_string()],
    )?;
    Ok(())
}
fn request(op: &str) -> Value {
    json!({"meeting_id":"meeting-1","operation_id":op,"expected_revision":0,
        "proposal_revision":1,"expected_goal_revision":0})
}
fn send(root: &Path, id: &str, actor: &str, payload: Value) -> anyhow::Result<Value> {
    crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({"id":id,"module":"ctox","record_id":"project","command_type":"ctox.workjet.jour_fixe.todos.confirm",
            "payload":payload,"client_context":{"actor":{"id":actor,"role":"admin","is_admin":true}}}),
    )
}
fn denied(value: anyhow::Result<Value>) {
    assert!(
        value.is_err()
            || value
                .as_ref()
                .is_ok_and(|v| v["status"] == "failed" || v["ok"] == false),
        "{value:?}"
    );
}
fn core(root: &Path) -> anyhow::Result<Connection> {
    Ok(Connection::open(crate::paths::core_db(root))?)
}
fn goals(root: &Path) -> anyhow::Result<i64> {
    let conn = core(root)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='planned_goals')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(0);
    }
    Ok(conn.query_row("SELECT COUNT(*) FROM planned_goals", [], |r| r.get(0))?)
}
fn read(root: &Path, id: &str, actor: &str) -> anyhow::Result<Value> {
    store::accept_rxdb_business_command_with_origin(
        root,
        json!({"id":id,"module":"ctox","record_id":"project",
        "command_type":"ctox.workjet.jour_fixe.meeting.read","payload":{"project_id":"project","meeting_id":"meeting-1"},
        "client_context":{"actor":{"id":actor,"role":"admin","is_admin":true}}}),
        store::CommandOrigin::TrustedLocal,
    )
}

#[test]
fn explicit_owner_confirmation_creates_one_exact_executable_core_goal_and_a_shared_receipt(
) -> anyhow::Result<()> {
    let root = fixture()?;
    assert_eq!(goals(root.path())?, 0);
    let result = send(root.path(), "confirm", "owner", request("confirm-op"))?;
    assert_eq!(result["status"], "completed", "{result}");
    let goal = result["result"]["goal"]["goal_id"]
        .as_str()
        .context("real goal id")?;
    assert_eq!(result["result"]["goal"]["revision"], 1);
    let conn = core(root.path())?;
    let (thread, active, automatic): (String, String, bool) = conn.query_row(
        "SELECT thread_key,status,auto_advance FROM planned_goals WHERE goal_id=?1",
        [goal],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    assert_eq!(
        thread,
        "business-os/threads/cc6cfe73-2824-4360-9daf-3b3efb079931"
    );
    assert_eq!(active, "active");
    assert!(automatic);
    let (instruction, defer): (String, Option<String>) = conn.query_row(
        "SELECT instruction,defer_until FROM planned_steps WHERE goal_id=?1",
        [goal],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert!(
        instruction.contains("Michael")
            && instruction.contains("1791800000000")
            && instruction.contains("saved answer survives reopening")
    );
    assert!(defer.is_none());
    assert_eq!(goals(root.path())?, 1);
    let value = read(root.path(), "read-confirmed", "owner")?;
    assert_eq!(value["result"]["meeting"]["state"], "confirmed");
    assert_eq!(
        value["result"]["meeting"]["todos"]["confirmed_by_user_id"],
        "owner"
    );
    assert_eq!(
        value["result"]["meeting"]["todos"]["goal"],
        result["result"]["goal"]
    );
    assert_eq!(value["result"]["current_goal"], result["result"]["goal"]);
    // There is no second authoritative Policy commit to fake atomicity.
    assert_eq!(
        super::jour_fixe_owner::saved(root.path())?["state"],
        "review"
    );
    assert!(!crate::business_os::domain_effect::contains(
        &open_store(root.path())?,
        "confirm"
    )?);
    assert!(crate::business_os::domain_effect::contains(
        &conn, "confirm"
    )?);
    let emitted = plan::emit_next_step_for_goal(root.path(), goal)?
        .context("real Core step was not runnable")?;
    // Core plans emit on their durable plan channel, not the queue channel
    // selected by load_queue_task. Verify the actual input, routing and intent.
    let (channel, routed_thread, direction, prompt, route): (String, String, String, String, String) = conn.query_row(
        "SELECT m.channel,m.thread_key,m.direction,m.body_text,r.route_status
         FROM communication_messages m JOIN communication_routing_state r USING(message_key)
         WHERE m.message_key=?1",
        [&emitted.message_key], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
    )?;
    assert_eq!(channel, "plan");
    assert_eq!(routed_thread, thread);
    assert_eq!(direction, "inbound");
    assert_eq!(route, "pending");
    assert!(prompt.contains("Michael") && prompt.contains("saved answer survives reopening"));
    assert!(plan::emit_next_step_for_goal(root.path(), goal)?.is_none());
    Ok(())
}

#[test]
fn command_and_operation_replays_keep_one_goal_without_reopening_completed_work(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let first = send(root.path(), "confirm", "owner", request("same-op"))?;
    let id = first["result"]["goal"]["goal_id"]
        .as_str()
        .context("goal")?;
    core(root.path())?.execute(
        "UPDATE planned_goals SET status='completed' WHERE goal_id=?1",
        [id],
    )?;
    for command in ["confirm", "different-command-same-op"] {
        let replay = send(root.path(), command, "owner", request("same-op"))?;
        assert_eq!(replay["result"], first["result"]);
    }
    assert_eq!(goals(root.path())?, 1);
    assert_eq!(
        core(root.path())?.query_row(
            "SELECT status FROM planned_goals WHERE goal_id=?1",
            [id],
            |r| r.get::<_, String>(0)
        )?,
        "completed"
    );
    let mut changed = request("same-op");
    changed["proposal_revision"] = json!(2);
    denied(send(root.path(), "conflicting-op", "owner", changed));
    assert_eq!(goals(root.path())?, 1);
    Ok(())
}

#[test]
fn stale_proposal_goal_or_meeting_revision_cannot_confirm_or_replace_a_definition(
) -> anyhow::Result<()> {
    for field in [
        "expected_revision",
        "proposal_revision",
        "expected_goal_revision",
    ] {
        let root = fixture()?;
        let mut payload = request("stale");
        payload[field] = json!(99);
        denied(send(root.path(), field, "owner", payload));
        assert_eq!(goals(root.path())?, 0);
    }
    Ok(())
}

#[test]
fn missing_owner_empty_acceptance_foreign_evidence_or_duplicate_todos_never_become_core_steps(
) -> anyhow::Result<()> {
    for kind in ["owner", "acceptance", "evidence", "duplicate", "empty"] {
        let root = fixture()?;
        let mut meeting = super::jour_fixe_owner::saved(root.path())?;
        match kind {
            "owner" => {
                meeting["todos"]["items"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("owner");
            }
            "acceptance" => meeting["todos"]["items"][0]["acceptance"] = json!("   "),
            "evidence" => {
                meeting["todos"]["items"][0]["evidence_ids"] = json!(["another-meeting-comment"])
            }
            "duplicate" => {
                let duplicate = meeting["todos"]["items"][0].clone();
                meeting["todos"]["items"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            _ => meeting["todos"]["items"] = json!([]),
        }
        save(root.path(), &meeting)?;
        denied(send(root.path(), kind, "owner", request(kind)));
        assert_eq!(goals(root.path())?, 0);
    }
    Ok(())
}

#[test]
fn foreign_actor_and_replaced_supervisor_binding_cannot_confirm_or_replay() -> anyhow::Result<()> {
    let root = fixture()?;
    denied(send(root.path(), "foreign", "foreign", request("foreign")));
    assert_eq!(goals(root.path())?, 0);
    let first = send(root.path(), "confirm", "owner", request("confirmed"))?;
    assert_eq!(first["status"], "completed");
    let conn = open_store(root.path())?;
    conn.execute(
        "DELETE FROM business_records WHERE collection='user_threads'",
        [],
    )?;
    drop(conn);
    denied(send(
        root.path(),
        "revoked-binding",
        "owner",
        request("confirmed"),
    ));
    denied(read(root.path(), "revoked-read", "owner"));
    assert_eq!(goals(root.path())?, 1);
    Ok(())
}

#[test]
fn core_receipt_failure_rolls_back_goal_steps_head_and_confirmation_together() -> anyhow::Result<()>
{
    let root = fixture()?;
    let conn = plan::confirmed_goal::open(root.path())?;
    conn.execute_batch(crate::business_os::domain_effect::SCHEMA)?;
    conn.execute_batch(
        "CREATE TRIGGER reject_core_confirmation BEFORE INSERT ON business_command_domain_effects
        BEGIN SELECT RAISE(ABORT,'injected Core receipt failure'); END;",
    )?;
    drop(conn);
    denied(send(
        root.path(),
        "receipt-fail",
        "owner",
        request("receipt-fail"),
    ));
    let conn = core(root.path())?;
    for table in [
        "planned_goals",
        "planned_steps",
        "workjet_project_goal_definitions",
        "workjet_jour_fixe_confirmations",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))?,
            0
        );
    }
    assert!(!crate::business_os::domain_effect::contains(
        &conn,
        "receipt-fail"
    )?);
    Ok(())
}

#[test]
fn lost_publication_ack_recovers_the_original_core_goal_and_confirmation() -> anyhow::Result<()> {
    let root = fixture()?;
    let rxdb = Connection::open(store::rxdb_store_path(root.path()))?;
    let schema: Value = serde_json::from_str(include_str!("../business_os_schema_contract.json"))?;
    let version = schema["business_commands"]["version"]
        .as_u64()
        .context("command schema")?;
    rxdb.execute_batch(&format!("CREATE TRIGGER reject_confirmation_ack BEFORE INSERT ON ctox_business_os__business_commands__v{version}
        WHEN NEW.id='lost-ack' AND json_extract(NEW.data,'$.status')='completed'
        BEGIN SELECT RAISE(ABORT,'injected confirmation ACK loss'); END;"))?;
    let name = format!("{}::confirmed_goal_crash_child", module_path!());
    let name = name.split_once("::").context("test name")?.1;
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("CTOX_TEST_CONFIRMED_GOAL_CRASH_ROOT", root.path())
        .spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("confirmation child missed its deadline: {result:?}");
            }
        }
    };
    assert_eq!(
        status.code(),
        Some(73),
        "child did not reach the committed Core effect"
    );
    assert_eq!(goals(root.path())?, 1);
    assert!(crate::business_os::domain_effect::contains(
        &core(root.path())?,
        "lost-ack"
    )?);
    rxdb.execute_batch("DROP TRIGGER reject_confirmation_ack")?;
    drop(rxdb);
    let recovered = crate::business_os::command_plane::recover_applied_domain_effect_for_intake(
        root.path(),
        "lost-ack",
    )?
    .context("Core effect did not recover")?;
    assert_eq!(recovered["status"], "completed");
    assert_eq!(goals(root.path())?, 1);
    assert_eq!(
        read(root.path(), "read-after-ack-loss", "owner")?["result"]["meeting"]["todos"]["goal"],
        recovered["result"]["goal"]
    );
    Ok(())
}

#[test]
fn confirmed_todos_cannot_be_revised_by_late_owner_or_supervisor_draft_writes() -> anyhow::Result<()>
{
    let root = fixture()?;
    let first = send(root.path(), "confirm", "owner", request("confirm"))?;
    let saved = super::jour_fixe_owner::saved(root.path())?;
    let items = saved["todos"]["items"].clone();
    denied(
        crate::business_os::command_plane::accept_rxdb_business_command(
            root.path(),
            json!({"id":"late-edit",
        "module":"ctox","record_id":"project","command_type":"ctox.workjet.jour_fixe.todos.revise",
        "payload":{"meeting_id":"meeting-1","operation_id":"late-edit","expected_revision":0,"proposal_revision":2,"items":items},
        "client_context":{"actor":{"id":"owner","role":"admin","is_admin":true}}}),
        ),
    );
    assert_eq!(
        read(root.path(), "still-confirmed", "owner")?["result"]["meeting"]["todos"]["goal"],
        first["result"]["goal"]
    );
    Ok(())
}

#[test]
fn next_preparation_retains_the_real_previous_core_goal_for_the_next_deck() -> anyhow::Result<()> {
    let root = fixture()?;
    let first = send(root.path(), "confirm", "owner", request("confirm"))?;
    let _ = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner",
        "Project owner",
        "admin",
        store::now_ms() as i64,
    )?;
    let conn = open_store(root.path())?;
    let mut project =
        outbound_load_record(&conn, "workjet_projects", "project")?.context("project")?;
    project["jour_fixe"] = json!({"weekday":1,"time":"13:00","timezone":"Europe/Berlin"});
    store::upsert_business_record(&conn, "workjet_projects", "project", 2, project)?;
    drop(conn);
    crate::business_os::reconcile_project_reports(root.path())?;
    let task = schedule::list_tasks(root.path())?
        .into_iter()
        .find(|v| v.name.starts_with("workjet-jour-fixe-prepare:"))
        .context("preparation schedule")?;
    let due =
        chrono::DateTime::parse_from_rfc3339(task.next_run_at.as_deref().context("schedule due")?)?
            .with_timezone(&chrono::Utc);
    assert_eq!(
        schedule::emit_due_task_at(root.path(), &task.task_id, due)?.emitted_count,
        1
    );
    let raw:String=open_store(root.path())?.query_row("SELECT metadata_json FROM workjet_jour_fixe_meetings WHERE meeting_id!='meeting-1' ORDER BY scheduled_at_ms DESC LIMIT 1",[],|r|r.get(0))?;
    let next: Value = serde_json::from_str(&raw)?;
    assert_eq!(next["previous_goal"], first["result"]["goal"]);
    let next: crate::business_os::workjet_jour_fixe_contract::Meeting =
        serde_json::from_value(next)?;
    let definition =
        super::super::jour_fixe_confirmed_goal::previous_goal_for_deck(root.path(), &next)?;
    assert_eq!(definition["goal"], first["result"]["goal"]);
    assert_eq!(definition["items"][0]["owner"], "Michael");
    assert_eq!(definition["steps"][0]["status"], "pending");
    assert_eq!(goals(root.path())?, 1);
    Ok(())
}

#[test]
fn a_later_meeting_supersedes_the_old_definition_and_older_confirmations_cannot_win(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let first = send(root.path(), "confirm-first", "owner", request("first"))?;
    let old = first["result"]["goal"]["goal_id"]
        .as_str()
        .context("old goal")?;
    let mut later = super::jour_fixe_owner::saved(root.path())?;
    later["id"] = json!("meeting-2");
    later["todos"]["meeting_id"] = json!("meeting-2");
    for name in ["slides", "comments", "transcript"] {
        for item in later[name].as_array_mut().unwrap() {
            item["meeting_id"] = json!("meeting-2");
        }
    }
    later["scheduled_at_ms"] = json!(1792054800000i64);
    later["prepare_at_ms"] = json!(1792047600000i64);
    let conn = open_store(root.path())?;
    conn.execute("INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-2','project','owner',1792054800000,?1,NULL)",[later.to_string()])?;
    drop(conn);
    let mut payload = request("second");
    payload["meeting_id"] = json!("meeting-2");
    payload["expected_goal_revision"] = json!(1);
    let second = send(root.path(), "confirm-second", "owner", payload)?;
    assert_eq!(second["status"], "completed", "{second}");
    assert_eq!(second["result"]["goal"]["revision"], 2);
    assert_eq!(
        core(root.path())?.query_row(
            "SELECT status FROM planned_goals WHERE goal_id=?1",
            [old],
            |r| r.get::<_, String>(0)
        )?,
        "superseded"
    );
    let replay = send(root.path(), "first-replayed", "owner", request("first"))?;
    assert_eq!(replay["result"], first["result"]);
    assert_eq!(goals(root.path())?, 2);
    let mut stale = request("new-old-op");
    stale["expected_goal_revision"] = json!(2);
    denied(send(root.path(), "late-old-meeting", "owner", stale));
    assert_eq!(goals(root.path())?, 2);
    Ok(())
}

#[test]
fn confirmed_goal_crash_child() -> anyhow::Result<()> {
    let Some(path) = std::env::var_os("CTOX_TEST_CONFIRMED_GOAL_CRASH_ROOT") else {
        return Ok(());
    };
    let root = Path::new(&path);
    let error = send(root, "lost-ack", "owner", request("lost-ack-op"))
        .expect_err("publication must fail after Core commit");
    assert!(format!("{error:#}").contains("injected confirmation ACK loss"));
    assert_eq!(goals(root)?, 1);
    std::process::exit(73);
}

#[test]
fn concurrent_owner_confirmations_linearize_to_one_goal_for_one_proposal() -> anyhow::Result<()> {
    let root = fixture()?;
    let barrier = Arc::new(Barrier::new(2));
    let results = std::thread::scope(|scope| {
        let workers = (0..2)
            .map(|index| {
                let barrier = barrier.clone();
                let path = root.path();
                scope.spawn(move || {
                    barrier.wait();
                    send(
                        path,
                        &format!("race-{index}"),
                        "owner",
                        request(&format!("race-op-{index}")),
                    )
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("confirmation worker panicked"))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results
            .iter()
            .filter(|result| result
                .as_ref()
                .is_ok_and(|value| value["status"] == "completed"))
            .count(),
        1,
        "{results:?}"
    );
    assert_eq!(goals(root.path())?, 1);
    assert_eq!(
        core(root.path())?.query_row(
            "SELECT COUNT(*) FROM workjet_jour_fixe_confirmations",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}
