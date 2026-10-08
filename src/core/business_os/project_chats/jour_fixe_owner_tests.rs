// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
pub(super) fn fixture(state: &str) -> anyhow::Result<TempDir> {
    let root = super::supervisor_turns::fixture()?;
    let corpus: Value = serde_json::from_str(include_str!(
        "../../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"
    ))?;
    let mut meeting = corpus["valid_cases"][0]["value"].clone();
    meeting["project_id"] = json!("project");
    meeting["owner_user_id"] = json!("owner");
    meeting["supervisor"] = json!({"workjet_thread_id":THREAD,"ctox_thread_key":format!("business-os/threads/{THREAD}")});
    meeting["state"] = json!(state);
    meeting["revision"] = json!(0);
    meeting["comments"] = json!([]);
    meeting["transcript"] = json!([]);
    meeting["todos"] = Value::Null;
    let conn = open_store(root.path())?;
    conn.execute_batch(super::super::jour_fixe_preparation::SCHEMA)?;
    conn.execute("INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-1','project','owner',1791450000000,?1,NULL)", [meeting.to_string()])?;
    Ok(root)
}
fn send(
    root: &Path,
    command: &str,
    action: &str,
    actor: &str,
    payload: Value,
) -> anyhow::Result<Value> {
    crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({"id":command,"module":"ctox","record_id":"project",
            "command_type":format!("ctox.workjet.jour_fixe.{action}"),"payload":payload,
            "client_context":{"actor":{"id":actor,"role":"admin","is_admin":true}}}),
    )
}
fn request(operation: &str, revision: u64) -> Value {
    json!({"meeting_id":"meeting-1","operation_id":operation,"expected_revision":revision})
}
fn rejected(value: anyhow::Result<Value>) {
    assert!(
        value.is_err()
            || value
                .as_ref()
                .is_ok_and(|v| v["status"] == "failed" || v["ok"] == false),
        "{value:?}"
    );
}
pub(super) fn saved(root: &Path) -> anyhow::Result<Value> {
    let raw: String = open_store(root)?.query_row(
        "SELECT metadata_json FROM workjet_jour_fixe_meetings WHERE meeting_id='meeting-1'",
        [],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
#[test]
fn owner_meeting_cycle_and_operation_replay_preserve_native_revision() -> anyhow::Result<()> {
    let root = fixture("ready")?;
    let first = send(
        root.path(),
        "start",
        "meeting.start",
        "owner",
        request("start-op", 0),
    )?;
    assert_eq!(first["status"], "completed");
    assert_eq!(first["result"]["mutation"]["state"], "live");
    assert_eq!(first["result"]["mutation"]["revision"], 1);
    let replay = send(
        root.path(),
        "start-replay",
        "meeting.start",
        "owner",
        request("start-op", 0),
    )?;
    assert_eq!(replay["result"], first["result"]);
    let ended = send(
        root.path(),
        "end",
        "meeting.end",
        "owner",
        request("end-op", 1),
    )?;
    assert_eq!(ended["status"], "completed");
    assert_eq!(saved(root.path())?["state"], "review");
    assert_eq!(saved(root.path())?["revision"], 2);
    rejected(send(
        root.path(),
        "changed-operation",
        "meeting.end",
        "owner",
        request("start-op", 2),
    ));
    assert_eq!(saved(root.path())?["revision"], 2);
    Ok(())
}
#[test]
fn unavailable_foreign_stale_and_nonready_meetings_do_not_transition() -> anyhow::Result<()> {
    for state in [
        "planned",
        "preparing",
        "live",
        "review",
        "confirmed",
        "cancelled",
        "failed",
    ] {
        let root = fixture(state)?;
        rejected(send(
            root.path(),
            "not-ready",
            "meeting.start",
            "owner",
            request("op", 0),
        ));
        assert_eq!(saved(root.path())?["state"], state);
    }
    let root = fixture("ready")?;
    rejected(send(
        root.path(),
        "foreign",
        "meeting.start",
        "foreign",
        request("foreign", 0),
    ));
    rejected(send(
        root.path(),
        "stale",
        "meeting.start",
        "owner",
        request("stale", 9),
    ));
    let mut missing = request("missing", 0);
    missing["meeting_id"] = json!("other-meeting");
    rejected(send(
        root.path(),
        "missing",
        "meeting.start",
        "owner",
        missing,
    ));
    assert_eq!(saved(root.path())?["revision"], 0);
    Ok(())
}
fn text_turn(operation: &str, revision: u64) -> Value {
    let mut value = request(operation, revision);
    value["turn"] = json!({"id":"owner-turn","meeting_id":"meeting-1","sequence":1,
        "speaker":"owner","modality":"text","text":"Please verify persistence.",
        "started_at_ms":1791450011000i64,"ended_at_ms":1791450012000i64});
    value
}
#[test]
fn owner_text_is_durable_idempotent_and_cannot_claim_supervisor_or_speech_provenance(
) -> anyhow::Result<()> {
    let root = fixture("live")?;
    let first = send(
        root.path(),
        "text",
        "transcript.append",
        "owner",
        text_turn("text-op", 0),
    )?;
    assert_eq!(first["status"], "completed");
    assert_eq!(
        saved(root.path())?["transcript"][0]["text"],
        "Please verify persistence."
    );
    let replay = send(
        root.path(),
        "text-replay",
        "transcript.append",
        "owner",
        text_turn("text-op", 0),
    )?;
    assert_eq!(replay["result"], first["result"]);
    assert_eq!(
        saved(root.path())?["transcript"].as_array().unwrap().len(),
        1
    );
    for (field, value) in [
        ("speaker", json!("supervisor")),
        ("modality", json!("speech")),
        ("source_run_id", json!("invented-stt")),
        ("stream_id", json!("invented-stream")),
        ("meeting_id", json!("another-meeting")),
        ("sequence", json!(5)),
        ("ended_at_ms", json!(1)),
    ] {
        let mut payload = text_turn(&format!("bad-{field}"), 1);
        payload["turn"]["id"] = json!(format!("turn-{field}"));
        payload["turn"][field] = value;
        rejected(send(
            root.path(),
            &format!("bad-{field}"),
            "transcript.append",
            "owner",
            payload,
        ));
    }
    assert_eq!(
        saved(root.path())?["transcript"].as_array().unwrap().len(),
        1
    );
    assert_eq!(saved(root.path())?["revision"], 1);
    Ok(())
}
#[test]
fn owner_todo_revision_stays_proposed_and_references_only_this_meeting() -> anyhow::Result<()> {
    let root = fixture("review")?;
    let conn = open_store(root.path())?;
    let mut meeting = saved(root.path())?;
    meeting["todos"] =
        json!({"revision":1,"status":"proposed","meeting_id":"meeting-1","items":[]});
    conn.execute(
        "UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",
        [meeting.to_string()],
    )?;
    let mut payload = request("revise", 0);
    payload["proposal_revision"] = json!(2);
    payload["items"] = json!([{"id":"todo","title":"Verify persistence","acceptance":"Survives reopen","priority":"P1","evidence_ids":["slide-1"]}]);
    let result = send(
        root.path(),
        "revise",
        "todos.revise",
        "owner",
        payload.clone(),
    )?;
    assert_eq!(result["status"], "completed");
    let meeting = saved(root.path())?;
    assert_eq!(meeting["todos"]["status"], "proposed");
    assert_eq!(meeting["todos"]["revision"], 2);
    assert!(meeting["todos"]["confirmed_by_user_id"].is_null());
    assert!(meeting["todos"]["goal"].is_null());
    payload["operation_id"] = json!("foreign-evidence");
    payload["expected_revision"] = json!(1);
    payload["proposal_revision"] = json!(3);
    payload["items"][0]["evidence_ids"] = json!(["foreign-turn"]);
    rejected(send(
        root.path(),
        "foreign-evidence",
        "todos.revise",
        "owner",
        payload,
    ));
    assert_eq!(saved(root.path())?["revision"], 1);
    Ok(())
}
#[test]
fn changed_supervisor_binding_and_extra_caller_authority_fail_closed() -> anyhow::Result<()> {
    let root = fixture("ready")?;
    let mut forged = request("forged", 0);
    forged["owner_user_id"] = json!("owner");
    rejected(send(
        root.path(),
        "forged",
        "meeting.start",
        "owner",
        forged,
    ));
    let conn = open_store(root.path())?;
    let mut thread = outbound_load_record(&conn, THREADS, THREAD)?.unwrap();
    thread["source_record_id"] = json!("another-project");
    store::upsert_business_record(&conn, THREADS, THREAD, 7, thread)?;
    rejected(send(
        root.path(),
        "wrong-binding",
        "meeting.start",
        "owner",
        request("binding", 0),
    ));
    assert_eq!(saved(root.path())?["revision"], 0);
    Ok(())
}
#[test]
fn domain_receipt_failure_rolls_back_the_meeting_mutation() -> anyhow::Result<()> {
    let root = fixture("ready")?;
    let conn = open_store(root.path())?;
    conn.execute_batch("CREATE TRIGGER fail_meeting_receipt BEFORE INSERT ON business_command_domain_effects WHEN NEW.command_id='receipt-fail' BEGIN SELECT RAISE(FAIL,'fixture receipt failure'); END;")?;
    rejected(send(
        root.path(),
        "receipt-fail",
        "meeting.start",
        "owner",
        request("receipt-op", 0),
    ));
    assert_eq!(saved(root.path())?["revision"], 0);
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM business_command_domain_effects WHERE command_id='receipt-fail'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(count, 0);
    Ok(())
}

#[test]
fn meeting_command_visibility_uses_its_native_owner_and_project_binding() -> anyhow::Result<()> {
    let root = fixture("ready")?;
    let document = json!({"id":"meeting-command","command_type":"ctox.workjet.jour_fixe.meeting.start",
        "record_id":"project","payload":request("start",0)});
    assert_eq!(
        document_visible_to_actor(root.path(), "business_commands", &document, "owner"),
        Some(true)
    );
    assert_eq!(
        document_visible_to_actor(root.path(), "business_commands", &document, "foreign"),
        Some(false)
    );
    let mut wrong = document.clone();
    wrong["record_id"] = json!("another-project");
    assert_eq!(
        document_visible_to_actor(root.path(), "business_commands", &wrong, "owner"),
        Some(false)
    );
    wrong = document;
    wrong["payload"]["meeting_id"] = json!("foreign-meeting");
    assert_eq!(
        document_visible_to_actor(root.path(), "business_commands", &wrong, "owner"),
        Some(false)
    );
    Ok(())
}

#[test]
fn reserved_meeting_tools_fail_terminally_without_creating_recursive_model_tasks(
) -> anyhow::Result<()> {
    let root = fixture("ready")?;
    for (i, action) in ["prepare", "deck.publish", "todos.propose", "todos.confirm"]
        .iter()
        .enumerate()
    {
        let command = format!("reserved-{i}");
        let error = send(root.path(), &command, action, "owner", request(&command, 0))
            .expect_err("reserved action must return its persisted failure");
        assert!(error.to_string().contains("not implemented"), "{error:#}");
        let value = store::load_rxdb_collection_record(root.path(), "business_commands", &command)?
            .expect("terminal failure must be persisted");
        assert_eq!(value["status"], "failed", "{value}");
        assert_eq!(value["result"]["ok"], false);
        assert!(value["result"]["error"]
            .as_str()
            .unwrap()
            .contains("not implemented"));
        assert!(
            crate::mission::channels::load_queue_task_for_business_os_command(
                root.path(),
                &command
            )?
            .is_none()
        );
    }
    assert_eq!(saved(root.path())?["revision"], 0);
    Ok(())
}

#[test]
fn live_stream_binding_is_read_only_and_revalidates_after_an_independent_writer(
) -> anyhow::Result<()> {
    use super::super::jour_fixe_owner::check_live_meeting_for_authenticated_actor;
    let root = fixture("live")?;
    let binding = check_live_meeting_for_authenticated_actor(
        root.path(),
        "owner",
        "project",
        "meeting-1",
        1,
    )?;
    assert_eq!(binding.owner_user_id(), "owner");
    assert_eq!(binding.project_id(), "project");
    assert_eq!(binding.meeting_id(), "meeting-1");
    assert_eq!(binding.deck_revision(), 1);
    assert_eq!(binding.meeting_revision(), 0);
    let mut metadata = saved(root.path())?;
    let mut conn = open_store(root.path())?;
    conn.busy_timeout(std::time::Duration::ZERO)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    metadata["revision"] = json!(1);
    tx.execute(
        "UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",
        [metadata.to_string()],
    )?;
    tx.commit()?;
    let refreshed = binding.revalidate(root.path(), "owner")?;
    assert_eq!(refreshed.meeting_revision(), 1);
    assert!(binding.revalidate(root.path(), "foreign").is_err());
    let ended = send(
        root.path(),
        "end-stream",
        "meeting.end",
        "owner",
        request("end-stream-op", 1),
    )?;
    assert_eq!(ended["status"], "completed");
    assert!(binding.revalidate(root.path(), "owner").is_err());
    Ok(())
}

#[test]
fn stream_binding_rejects_foreign_project_deck_state_and_missing_store_without_creation(
) -> anyhow::Result<()> {
    use super::super::jour_fixe_owner::check_live_meeting_for_authenticated_actor;
    let root = fixture("live")?;
    for (actor, project, meeting, deck) in [
        ("foreign", "project", "meeting-1", 1),
        ("owner", "foreign", "meeting-1", 1),
        ("owner", "project", "foreign", 1),
        ("owner", "project", "meeting-1", 0),
        ("owner", "project", "meeting-1", 2),
    ] {
        assert!(check_live_meeting_for_authenticated_actor(
            root.path(),
            actor,
            project,
            meeting,
            deck
        )
        .is_err());
    }
    for state in [
        "planned",
        "preparing",
        "ready",
        "review",
        "confirmed",
        "cancelled",
        "failed",
    ] {
        let root = fixture(state)?;
        assert!(check_live_meeting_for_authenticated_actor(
            root.path(),
            "owner",
            "project",
            "meeting-1",
            1
        )
        .is_err());
    }
    let empty = TempDir::new()?;
    let path = store::business_os_store_path(empty.path());
    assert!(!path.exists());
    assert!(check_live_meeting_for_authenticated_actor(
        empty.path(),
        "owner",
        "project",
        "meeting-1",
        1
    )
    .is_err());
    assert!(
        !path.exists(),
        "metadata checks must not create or migrate a store"
    );
    Ok(())
}

#[test]
fn live_binding_rechecks_current_owner_supervisor_and_deck_after_provider_wait(
) -> anyhow::Result<()> {
    use super::super::jour_fixe_owner::check_live_meeting_for_authenticated_actor;
    for changed in ["owner", "supervisor", "deck"] {
        let root = fixture("live")?;
        let binding = check_live_meeting_for_authenticated_actor(
            root.path(),
            "owner",
            "project",
            "meeting-1",
            1,
        )?;
        let conn = open_store(root.path())?;
        if changed == "supervisor" {
            let mut thread = outbound_load_record(&conn, THREADS, THREAD)?.unwrap();
            thread["source_record_id"] = json!("foreign-project");
            store::upsert_business_record(&conn, THREADS, THREAD, 8, thread)?;
        } else if changed == "owner" {
            let mut project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
            project["owner_user_id"] = json!("foreign");
            store::upsert_business_record(&conn, "workjet_projects", "project", 8, project)?;
        } else {
            let mut metadata = saved(root.path())?;
            metadata["deck_revision"] = json!(2);
            conn.execute(
                "UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",
                [metadata.to_string()],
            )?;
        }
        assert!(
            binding.revalidate(root.path(), "owner").is_err(),
            "{changed}"
        );
    }
    Ok(())
}

fn comment_request(operation: &str, revision: u64) -> Value {
    let mut value = request(operation, revision);
    value["comment_id"] = json!("comment-1");
    value["slide_id"] = json!("slide-1");
    value["deck_revision"] = json!(1);
    value["x"] = json!(0.25);
    value["y"] = json!(0.75);
    value["text"] = json!("Please prioritize persistence.");
    value
}
#[test]
fn owner_slide_comment_is_durable_bound_to_deck_and_operation_idempotent() -> anyhow::Result<()> {
    let root = fixture("live")?;
    let token = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner",
        "Owner",
        "admin",
        chrono::Utc::now().timestamp_millis(),
    )?
    .0;
    let first = crate::business_os::command_plane::accept_rxdb_business_command_with_origin(
        root.path(),
        json!({"id":"comment","module":"ctox","record_id":"project",
            "command_type":"ctox.workjet.jour_fixe.comment.add",
            "payload":comment_request("comment-op",0),
            "client_context":{"actor":{"id":"owner"},"capability_token":token}}),
        store::CommandOrigin::ReplicatedPeer,
    )?;
    assert_eq!(first["status"], "completed", "{first}");
    assert_eq!(first["result"]["mutation"]["changed_id"], "comment-1");
    let receipt: String = open_store(root.path())?.query_row(
        "SELECT receipt_json FROM business_command_domain_effects WHERE command_id='comment'",
        [],
        |r| r.get(0),
    )?;
    let mut terminal_result = serde_json::from_str::<Value>(&receipt)?["result"].clone();
    terminal_result["status"] = json!("completed");
    terminal_result["task_status"] = json!("completed");
    assert_eq!(terminal_result, first["result"]);
    let replay = send(
        root.path(),
        "comment-replay",
        "comment.add",
        "owner",
        comment_request("comment-op", 0),
    )?;
    assert_eq!(first["result"], replay["result"]);
    let meeting = saved(root.path())?;
    let comments = meeting["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["author_user_id"], "owner");
    assert_eq!(comments[0]["meeting_id"], "meeting-1");
    assert_eq!(comments[0]["deck_revision"], 1);
    assert_eq!(comments[0]["x"], 0.25);
    assert!(comments[0]["created_at_ms"].as_i64().unwrap() > 0);
    assert!(
        comments[0]["supervisor_event_id"].is_null(),
        "no invented supervisor response"
    );
    assert_eq!(meeting["revision"], 1);
    let mut changed = comment_request("comment-op", 1);
    changed["text"] = json!("Changed intent");
    rejected(send(
        root.path(),
        "changed-comment",
        "comment.add",
        "owner",
        changed,
    ));
    rejected(send(
        root.path(),
        "duplicate-id",
        "comment.add",
        "owner",
        comment_request("different-op", 1),
    ));
    assert_eq!(saved(root.path())?["comments"].as_array().unwrap().len(), 1);
    Ok(())
}
#[test]
fn comments_reject_foreign_actor_deck_slide_pin_author_and_closed_meetings() -> anyhow::Result<()> {
    let root = fixture("live")?;
    rejected(send(
        root.path(),
        "foreign-comment",
        "comment.add",
        "foreign",
        comment_request("foreign-op", 0),
    ));
    for (field, value) in [
        ("deck_revision", json!(2)),
        ("slide_id", json!("foreign-slide")),
        ("x", json!(-0.1)),
        ("y", json!(1.1)),
        ("comment_id", json!("slide-1")),
        ("author_user_id", json!("owner")),
        ("supervisor_event_id", json!("invented-event")),
        ("text", json!("  ")),
    ] {
        let mut payload = comment_request(&format!("bad-{field}"), 0);
        payload[field] = value;
        rejected(send(
            root.path(),
            &format!("bad-{field}"),
            "comment.add",
            "owner",
            payload,
        ));
    }
    assert!(saved(root.path())?["comments"]
        .as_array()
        .unwrap()
        .is_empty());
    for state in [
        "planned",
        "preparing",
        "ready",
        "confirmed",
        "cancelled",
        "failed",
    ] {
        let root = fixture(state)?;
        rejected(send(
            root.path(),
            "closed-comment",
            "comment.add",
            "owner",
            comment_request("closed-op", 0),
        ));
        assert!(saved(root.path())?["comments"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    Ok(())
}
#[test]
fn comment_rolls_back_with_failed_domain_receipt_and_retries_once() -> anyhow::Result<()> {
    let root = fixture("review")?;
    let conn = open_store(root.path())?;
    conn.execute_batch("CREATE TRIGGER fail_comment_receipt BEFORE INSERT ON business_command_domain_effects BEGIN SELECT RAISE(FAIL,'comment receipt fixture failure'); END;")?;
    rejected(send(
        root.path(),
        "failed-comment",
        "comment.add",
        "owner",
        comment_request("comment-op", 0),
    ));
    assert!(saved(root.path())?["comments"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(saved(root.path())?["revision"], 0);
    conn.execute_batch("DROP TRIGGER fail_comment_receipt")?;
    let retried = send(
        root.path(),
        "retried-comment",
        "comment.add",
        "owner",
        comment_request("comment-op", 0),
    )?;
    assert_eq!(retried["status"], "completed", "{retried}");
    assert_eq!(saved(root.path())?["comments"].as_array().unwrap().len(), 1);
    Ok(())
}
