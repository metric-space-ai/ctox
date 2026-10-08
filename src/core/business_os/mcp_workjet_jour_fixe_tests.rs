// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use serde_json::json;

const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
fn fixture(state: &str) -> anyhow::Result<(tempfile::TempDir,Value)> {
    let (root,trusted) = workjet_worker_dispatch::meeting_test_fixture()?;
    let corpus: Value = serde_json::from_str(include_str!("../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"))?;
    let mut meeting = corpus["valid_cases"][0]["value"].clone();
    meeting["project_id"] = json!("project"); meeting["owner_user_id"] = json!("owner");
    meeting["supervisor"] = json!({"workjet_thread_id":THREAD,"ctox_thread_key":format!("business-os/threads/{THREAD}")});
    meeting["state"] = json!(state); meeting["revision"] = json!(0);
    meeting["comments"] = json!([]); meeting["transcript"] = json!([]); meeting["todos"] = Value::Null;
    if state == "planned" { meeting["slides"] = json!([]); meeting["deck_revision"] = json!(0); }
    let policy = store::open_store(root.path())?;
    policy.execute_batch("CREATE TABLE workjet_jour_fixe_meetings (
        meeting_id TEXT PRIMARY KEY,project_id TEXT NOT NULL,owner_user_id TEXT NOT NULL,
        scheduled_at_ms INTEGER NOT NULL,metadata_json TEXT NOT NULL,preparation_task_id TEXT);")?;
    policy.execute("INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-1','project','owner',1791450000000,?1,NULL)",[meeting.to_string()])?;
    Ok((root,trusted))
}
fn call(root: &Path,trusted: &Value,tool: &str,args: Value) -> anyhow::Result<Value> {
    super::super::call_tool_inner(root,tool,args,Some(trusted))
}
fn read_args(action: &str) -> Value {
    json!({"action":action,"request":{"project_id":"project","meeting_id":"meeting-1"}})
}
fn draft() -> Value {
    json!({"action":"prepare_deck","request":{"operation_id":"draft-op","meeting_id":"meeting-1",
        "expected_revision":0,"deck_revision":1,"slides":[{"id":"draft-slide","position":0,
        "title":"Progress","body_markdown":"Evidence and owner decisions","meeting_id":"meeting-1"}]}})
}
fn proposal() -> Value {
    json!({"action":"propose_todos","request":{"operation_id":"proposal-op","meeting_id":"meeting-1",
        "expected_revision":0,"proposal_revision":1,"items":[{"id":"todo-1","title":"Verify reopen",
        "acceptance":"Saved data survives reopen","priority":"P1","evidence_ids":["slide-1"],
        "owner":"Project supervisor","due_at_ms":1791450000000}]}})
}
fn saved(root: &Path) -> anyhow::Result<Value> {
    let raw: String = Connection::open_with_flags(store::business_os_store_path(root),OpenFlags::SQLITE_OPEN_READ_ONLY)?
        .query_row("SELECT metadata_json FROM workjet_jour_fixe_meetings WHERE meeting_id='meeting-1'",[],|r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
fn save(root: &Path,meeting: &Value) -> anyhow::Result<()> {
    store::open_store(root)?.execute("UPDATE workjet_jour_fixe_meetings SET metadata_json=?1 WHERE meeting_id='meeting-1'",[meeting.to_string()])?;
    Ok(())
}
fn core(root: &Path) -> anyhow::Result<Connection> { Ok(Connection::open(crate::paths::core_db(root))?) }

#[test]
fn signed_supervisor_draft_persists_once_but_never_claims_ready_or_audio() -> anyhow::Result<()> {
    let (root,trusted) = fixture("planned")?;
    let args = draft();
    let first = call(root.path(),&trusted,WRITE_TOOL,args.clone())?;
    assert_eq!(first["mutation"]["state"],"preparing");
    assert_eq!(first["mutation"]["revision"],1);
    assert_eq!(call(root.path(),&trusted,WRITE_TOOL,args.clone())?,first);
    assert_eq!(saved(root.path())?["slides"][0]["id"],"draft-slide");
    assert_eq!(saved(root.path())?["slides"][0]["audio"],Value::Null);
    let mut changed = args; changed["request"]["slides"][0]["title"] = json!("Different");
    assert!(call(root.path(),&trusted,WRITE_TOOL,changed).is_err());
    assert_eq!(saved(root.path())?["revision"],1);
    assert_eq!(store::open_store(root.path())?.query_row("SELECT count(*) FROM workjet_jour_fixe_supervisor_operations",[],|r|r.get::<_,i64>(0))?,1);
    Ok(())
}
#[test]
fn draft_rejects_forged_narration_wrong_order_links_and_strict_fields() -> anyhow::Result<()> {
    for change in ["audio","order","link","duplicate","blank","unknown","oversized"] {
        let (root,trusted) = fixture("planned")?;
        let mut args = draft();
        match change {
            "audio" => {
                let corpus: Value = serde_json::from_str(include_str!("../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"))?;
                args["request"]["slides"][0]["audio"] = corpus["valid_cases"][0]["value"]["slides"][0]["audio"].clone();
            },
            "order" => args["request"]["slides"][0]["position"] = json!(1),
            "link" => args["request"]["slides"][0]["meeting_id"] = json!("foreign-meeting"),
            "duplicate" => { let slide=args["request"]["slides"][0].clone(); args["request"]["slides"].as_array_mut().unwrap().push(slide); },
            "blank" => args["request"]["slides"][0]["body_markdown"] = json!(" "),
            "unknown" => args["request"]["owner_user_id"] = json!("owner"),
            "oversized" => args["request"]["slides"][0]["body_markdown"] = json!("x".repeat(16385)),
            _ => unreachable!(),
        };
        assert!(call(root.path(),&trusted,WRITE_TOOL,args).is_err(),"{change}");
        assert_eq!(saved(root.path())?["revision"],0);
    }
    Ok(())
}
#[test]
fn proposal_has_owner_evidence_due_and_replays_without_installing_a_goal() -> anyhow::Result<()> {
    let (root,trusted) = fixture("review")?;
    let first = call(root.path(),&trusted,WRITE_TOOL,proposal())?;
    assert_eq!(first["mutation"]["todos_revision"],1);
    assert_eq!(call(root.path(),&trusted,WRITE_TOOL,proposal())?,first);
    let meeting = saved(root.path())?;
    assert_eq!(meeting["state"],"review");
    assert_eq!(meeting["todos"]["status"],"proposed");
    assert_eq!(meeting["todos"]["items"][0]["owner"],"Project supervisor");
    assert_eq!(meeting["todos"]["items"][0]["due_at_ms"],1791450000000_i64);
    assert_eq!(meeting["todos"]["goal"],Value::Null);
    assert_eq!(meeting["todos"]["confirmed_by_user_id"],Value::Null);
    Ok(())
}
#[test]
fn proposal_rejects_missing_owner_foreign_evidence_stale_revision_and_confirmed_goal() -> anyhow::Result<()> {
    for change in ["owner","evidence","duplicate","revision","expected","state","confirmed"] {
        let (root,trusted) = fixture("review")?;
        let mut args = proposal();
        match change {
            "owner" => {args["request"]["items"][0].as_object_mut().unwrap().remove("owner");},
            "evidence" => args["request"]["items"][0]["evidence_ids"] = json!(["other-meeting-comment"]),
            "duplicate" => {let todo=args["request"]["items"][0].clone();args["request"]["items"].as_array_mut().unwrap().push(todo);},
            "revision" => args["request"]["proposal_revision"] = json!(2),
            "expected" => args["request"]["expected_revision"] = json!(9),
            "state" => {let mut m=saved(root.path())?;m["state"]=json!("live");save(root.path(),&m)?;},
            "confirmed" => {let mut m=saved(root.path())?;m["todos"]=json!({"meeting_id":"meeting-1","revision":1,"status":"confirmed","items":[],
                "goal":{"goal_id":"real-prior-goal","revision":1},"confirmed_by_user_id":"owner","confirmed_at_ms":1});save(root.path(),&m)?;},
            _ => unreachable!(),
        };
        assert!(call(root.path(),&trusted,WRITE_TOOL,args).is_err(),"{change}");
        assert_eq!(saved(root.path())?["revision"],0);
    }
    Ok(())
}
#[test]
fn ordinary_managed_owner_and_foreign_peer_cannot_impersonate_supervisor() -> anyhow::Result<()> {
    let (root,trusted) = fixture("planned")?;
    for actor in ["owner","foreign"] {
        let gateway=json!({"auth_source":"ctox_dev_managed_mcp_token","channel":"ctox_dev_managed_mcp",
            "surface":"workjet","actor":actor,"role":"chef","workspace":"tenant:source-owner","instance_id":"source-instance"});
        assert!(call(root.path(),&gateway,WRITE_TOOL,draft()).is_err());
        assert!(call(root.path(),&gateway,READ_TOOL,read_args("read_comments")).is_err());
    }
    // A valid token for this owner/project still cannot touch another meeting.
    let mut meeting=saved(root.path())?;
    meeting["project_id"]=json!("another-project");save(root.path(),&meeting)?;
    assert!(call(root.path(),&trusted,WRITE_TOOL,draft()).is_err());
    assert_eq!(saved(root.path())?["revision"],0);
    Ok(())
}
#[test]
fn replaced_expired_and_cancelled_native_lease_reject_reads_writes_and_replays() -> anyhow::Result<()> {
    for change in ["replaced","expired","cancelled","hash","terminal","epoch"] {
        let (root,mut trusted)=fixture("planned")?;
        call(root.path(),&trusted,WRITE_TOOL,draft())?;
        match change {
            "replaced" => {core(root.path())?.execute("UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE route_status='leased'",[])?;},
            "expired" => {core(root.path())?.execute("UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE route_status='leased'",[])?;},
            "cancelled" => {core(root.path())?.execute("UPDATE communication_routing_state SET route_status='cancelled' WHERE route_status='leased'",[])?;},
            // Claims below are altered only in this native seam test; production
            // callers cannot edit the verified signed token.
            "hash" => trusted["payload_hash"]=json!("wrong"),
            "terminal" => {core(root.path())?.execute("UPDATE business_command_aggregates SET execution_phase='terminal' WHERE command_id=?1",[required_arg(&trusted,"command_id")?])?;},
            "epoch" => trusted["workjet_supervisor_epoch"]=json!(-1),
            _=>unreachable!(),
        };
        assert!(call(root.path(),&trusted,WRITE_TOOL,draft()).is_err(),"{change}");
        assert!(call(root.path(),&trusted,READ_TOOL,read_args("read_transcript")).is_err(),"{change}");
        assert_eq!(saved(root.path())?["revision"],1);
    }
    Ok(())
}
#[test]
fn retained_comments_and_final_transcript_are_scoped_and_do_not_take_writer_locks() -> anyhow::Result<()> {
    let (root,trusted)=fixture("review")?;
    let corpus: Value=serde_json::from_str(include_str!("../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"))?;
    let mut meeting=saved(root.path())?;
    meeting["comments"]=corpus["valid_cases"][0]["value"]["comments"].clone();
    meeting["transcript"]=corpus["valid_cases"][0]["value"]["transcript"].clone();
    save(root.path(),&meeting)?;
    let comments=call(root.path(),&trusted,READ_TOOL,read_args("read_comments"))?;
    assert_eq!(comments["comments"],meeting["comments"]);
    assert!(comments.get("transcript").is_none());
    let transcript=call(root.path(),&trusted,READ_TOOL,read_args("read_transcript"))?;
    assert_eq!(transcript["transcript"],meeting["transcript"]);
    let mut args=read_args("read_comments");args["request"]["project_id"]=json!("other");
    assert!(call(root.path(),&trusted,READ_TOOL,args).is_err());
    // Native domain reads remain usable while both databases have unrelated
    // WAL writers. This directly exercises the bounded tool execution path.
    let mut c=core(root.path())?;let c_tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut p=store::open_store(root.path())?;let p_tx=p.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let context=context_from_arguments_with_trusted_gateway_context(READ_TOOL,&read_args("read_comments"),Some(&trusted))?;
    assert_eq!(execute(root.path(),&context,READ_TOOL,&read_args("read_comments"),Some(&trusted))?,comments);
    p_tx.rollback()?;c_tx.rollback()?;
    Ok(())
}
#[test]
fn supervisor_tool_allowlist_stays_bounded_and_mcp_write_policy_is_effective() -> anyhow::Result<()> {
    let (root,trusted)=fixture("planned")?;
    assert!(crew_only_session_allows_tool(READ_TOOL,Some(&trusted)));
    assert!(crew_only_session_allows_tool(WRITE_TOOL,Some(&trusted)));
    assert!(!crew_only_session_allows_tool("business_os.execute_action",Some(&trusted)));
    assert!(call(root.path(),&trusted,WRITE_TOOL,json!({"action":"confirm_todos","request":{}})).is_err());
    let mut policy=default_mcp_policy();policy.allow_writes=false;save_mcp_policy(root.path(),&policy)?;
    assert!(call(root.path(),&trusted,WRITE_TOOL,draft()).is_err());
    assert!(call(root.path(),&trusted,READ_TOOL,read_args("read_comments")).is_ok());
    assert_eq!(saved(root.path())?["revision"],0);
    Ok(())
}
#[test]
fn failed_metadata_commit_rolls_back_operation_receipt_and_cas() -> anyhow::Result<()> {
    let (root,trusted)=fixture("planned")?;
    let p=store::open_store(root.path())?;
    p.execute_batch("CREATE TRIGGER fail_meeting_update BEFORE UPDATE ON workjet_jour_fixe_meetings BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END;")?;
    assert!(call(root.path(),&trusted,WRITE_TOOL,draft()).is_err());
    assert_eq!(saved(root.path())?["revision"],0);
    let exists:bool=p.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_jour_fixe_supervisor_operations')",[],|r|r.get(0))?;
    assert!(!exists,"failed transaction must not publish a receipt or create its schema");
    p.execute_batch("DROP TRIGGER fail_meeting_update;")?;
    assert_eq!(call(root.path(),&trusted,WRITE_TOOL,draft())?["mutation"]["revision"],1);
    Ok(())
}

#[test]
fn meeting_tool_descriptors_are_strict_root_objects_from_the_shared_dtos() {
    let read = descriptor_schema(&[("read_comments","ReadMeetingRequest")]);
    assert_eq!(read["type"],"object");
    assert_eq!(read["additionalProperties"],false);
    assert_eq!(read["required"],json!(["action","request"]));
    assert_eq!(read["oneOf"][0]["properties"]["request"]["additionalProperties"],false);
    let update = descriptor_schema(&[("propose_todos","ProposeTodosRequest")]);
    let fields = &update["oneOf"][0]["properties"]["request"]["properties"]["items"]["items"]["properties"];
    assert_eq!(fields["owner"]["anyOf"][0]["maxLength"],256);
    assert_eq!(fields["due_at_ms"]["anyOf"][0]["minimum"],0);
}
