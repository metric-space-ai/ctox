// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
fn fixture(state: &str) -> anyhow::Result<TempDir> {
    let root = super::supervisor_turns::fixture()?;
    let corpus:Value = serde_json::from_str(include_str!("../../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"))?;
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
fn send(root:&Path, command:&str, action:&str, actor:&str, payload:Value) -> anyhow::Result<Value> {
    crate::business_os::command_plane::accept_rxdb_business_command(root,
        json!({"id":command,"module":"ctox","record_id":"project",
            "command_type":format!("ctox.workjet.jour_fixe.{action}"),"payload":payload,
            "client_context":{"actor":{"id":actor,"role":"admin","is_admin":true}}}))
}
fn request(operation:&str, revision:u64) -> Value {
    json!({"meeting_id":"meeting-1","operation_id":operation,"expected_revision":revision})
}
fn rejected(value:anyhow::Result<Value>) {
    assert!(value.is_err() || value.as_ref().is_ok_and(|v|v["status"]=="failed" || v["ok"]==false),"{value:?}");
}
fn saved(root:&Path) -> anyhow::Result<Value> {
    let raw:String = open_store(root)?.query_row("SELECT metadata_json FROM workjet_jour_fixe_meetings WHERE meeting_id='meeting-1'",[],|r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
#[test]
fn owner_meeting_cycle_and_operation_replay_preserve_native_revision() -> anyhow::Result<()> {
    let root=fixture("ready")?;
    let first=send(root.path(),"start","meeting.start","owner",request("start-op",0))?;
    assert_eq!(first["status"],"completed");
    assert_eq!(first["result"]["mutation"]["state"],"live");
    assert_eq!(first["result"]["mutation"]["revision"],1);
    let replay=send(root.path(),"start-replay","meeting.start","owner",request("start-op",0))?;
    assert_eq!(replay["result"],first["result"]);
    let ended=send(root.path(),"end","meeting.end","owner",request("end-op",1))?;
    assert_eq!(ended["status"],"completed");
    assert_eq!(saved(root.path())?["state"],"review");
    assert_eq!(saved(root.path())?["revision"],2);
    rejected(send(root.path(),"changed-operation","meeting.end","owner",request("start-op",2)));
    assert_eq!(saved(root.path())?["revision"],2);
    Ok(())
}
#[test]
fn unavailable_foreign_stale_and_nonready_meetings_do_not_transition() -> anyhow::Result<()> {
    for state in ["planned","preparing","live","review","confirmed","cancelled","failed"] {
        let root=fixture(state)?;
        rejected(send(root.path(),"not-ready","meeting.start","owner",request("op",0)));
        assert_eq!(saved(root.path())?["state"],state);
    }
    let root=fixture("ready")?;
    rejected(send(root.path(),"foreign","meeting.start","foreign",request("foreign",0)));
    rejected(send(root.path(),"stale","meeting.start","owner",request("stale",9)));
    let mut missing=request("missing",0);missing["meeting_id"]=json!("other-meeting");
    rejected(send(root.path(),"missing","meeting.start","owner",missing));
    assert_eq!(saved(root.path())?["revision"],0);
    Ok(())
}
fn text_turn(operation:&str,revision:u64)->Value {
    let mut value=request(operation,revision);
    value["turn"]=json!({"id":"owner-turn","meeting_id":"meeting-1","sequence":1,
        "speaker":"owner","modality":"text","text":"Please verify persistence.",
        "started_at_ms":1791450011000i64,"ended_at_ms":1791450012000i64});
    value
}
#[test]
fn owner_text_is_durable_idempotent_and_cannot_claim_supervisor_or_speech_provenance() -> anyhow::Result<()> {
    let root=fixture("live")?;
    let first=send(root.path(),"text","transcript.append","owner",text_turn("text-op",0))?;
    assert_eq!(first["status"],"completed");
    assert_eq!(saved(root.path())?["transcript"][0]["text"],"Please verify persistence.");
    let replay=send(root.path(),"text-replay","transcript.append","owner",text_turn("text-op",0))?;
    assert_eq!(replay["result"],first["result"]);
    assert_eq!(saved(root.path())?["transcript"].as_array().unwrap().len(),1);
    for (field,value) in [("speaker",json!("supervisor")),("modality",json!("speech")),
        ("source_run_id",json!("invented-stt")),("stream_id",json!("invented-stream")),
        ("meeting_id",json!("another-meeting")),("sequence",json!(5)),("ended_at_ms",json!(1))] {
        let mut payload=text_turn(&format!("bad-{field}"),1);
        payload["turn"]["id"]=json!(format!("turn-{field}"));
        payload["turn"][field]=value;
        rejected(send(root.path(),&format!("bad-{field}"),"transcript.append","owner",payload));
    }
    assert_eq!(saved(root.path())?["transcript"].as_array().unwrap().len(),1);
    assert_eq!(saved(root.path())?["revision"],1);
    Ok(())
}
#[test]
fn owner_todo_revision_stays_proposed_and_references_only_this_meeting() -> anyhow::Result<()> {
    let root=fixture("review")?;
    let conn=open_store(root.path())?;
    let mut meeting=saved(root.path())?;
    meeting["todos"]=json!({"revision":1,"status":"proposed","meeting_id":"meeting-1","items":[]});
    conn.execute("UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",[meeting.to_string()])?;
    let mut payload=request("revise",0);
    payload["proposal_revision"]=json!(2);
    payload["items"]=json!([{"id":"todo","title":"Verify persistence","acceptance":"Survives reopen","priority":"P1","evidence_ids":["slide-1"]}]);
    let result=send(root.path(),"revise","todos.revise","owner",payload.clone())?;
    assert_eq!(result["status"],"completed");
    let meeting=saved(root.path())?;
    assert_eq!(meeting["todos"]["status"],"proposed");
    assert_eq!(meeting["todos"]["revision"],2);
    assert!(meeting["todos"]["confirmed_by_user_id"].is_null());
    assert!(meeting["todos"]["goal"].is_null());
    payload["operation_id"]=json!("foreign-evidence");
    payload["expected_revision"]=json!(1);
    payload["proposal_revision"]=json!(3);
    payload["items"][0]["evidence_ids"]=json!(["foreign-turn"]);
    rejected(send(root.path(),"foreign-evidence","todos.revise","owner",payload));
    assert_eq!(saved(root.path())?["revision"],1);
    Ok(())
}
#[test]
fn changed_supervisor_binding_and_extra_caller_authority_fail_closed() -> anyhow::Result<()> {
    let root=fixture("ready")?;
    let mut forged=request("forged",0);forged["owner_user_id"]=json!("owner");
    rejected(send(root.path(),"forged","meeting.start","owner",forged));
    let conn=open_store(root.path())?;
    let mut thread=outbound_load_record(&conn,THREADS,THREAD)?.unwrap();
    thread["source_record_id"]=json!("another-project");
    store::upsert_business_record(&conn,THREADS,THREAD,7,thread)?;
    rejected(send(root.path(),"wrong-binding","meeting.start","owner",request("binding",0)));
    assert_eq!(saved(root.path())?["revision"],0);
    Ok(())
}
#[test]
fn domain_receipt_failure_rolls_back_the_meeting_mutation() -> anyhow::Result<()> {
    let root=fixture("ready")?;
    let conn=open_store(root.path())?;
    conn.execute_batch("CREATE TRIGGER fail_meeting_receipt BEFORE INSERT ON business_command_domain_effects WHEN NEW.command_id='receipt-fail' BEGIN SELECT RAISE(FAIL,'fixture receipt failure'); END;")?;
    rejected(send(root.path(),"receipt-fail","meeting.start","owner",request("receipt-op",0)));
    assert_eq!(saved(root.path())?["revision"],0);
    let count:i64=conn.query_row("SELECT count(*) FROM business_command_domain_effects WHERE command_id='receipt-fail'",[],|r|r.get(0))?;
    assert_eq!(count,0);
    Ok(())
}

#[test]
fn meeting_command_visibility_uses_its_native_owner_and_project_binding() -> anyhow::Result<()> {
    let root=fixture("ready")?;
    let document=json!({"id":"meeting-command","command_type":"ctox.workjet.jour_fixe.meeting.start",
        "record_id":"project","payload":request("start",0)});
    assert_eq!(document_visible_to_actor(root.path(),"business_commands",&document,"owner"),Some(true));
    assert_eq!(document_visible_to_actor(root.path(),"business_commands",&document,"foreign"),Some(false));
    let mut wrong=document.clone();wrong["record_id"]=json!("another-project");
    assert_eq!(document_visible_to_actor(root.path(),"business_commands",&wrong,"owner"),Some(false));
    wrong=document;wrong["payload"]["meeting_id"]=json!("foreign-meeting");
    assert_eq!(document_visible_to_actor(root.path(),"business_commands",&wrong,"owner"),Some(false));
    Ok(())
}
