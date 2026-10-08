// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use serde_json::json;
const THREAD:&str="cc6cfe73-2824-4360-9daf-3b3efb079931";
pub(super) fn fixture()->anyhow::Result<(tempfile::TempDir,String)> {
    let (root, _) = workjet_worker_dispatch::meeting_test_fixture()?;
    let corpus:Value=serde_json::from_str(include_str!("../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"))?;
    let mut meeting=corpus["valid_cases"][0]["value"].clone();
    meeting["project_id"]=json!("project");meeting["owner_user_id"]=json!("owner");
    meeting["supervisor"]=json!({"workjet_thread_id":THREAD,"ctox_thread_key":format!("business-os/threads/{THREAD}")});
    meeting["state"]=json!("review");meeting["revision"]=json!(0);
    meeting["comments"]=json!([]);meeting["transcript"]=json!([]);meeting["previous_goal"]=Value::Null;
    meeting["todos"]=json!({"meeting_id":"meeting-1","revision":1,"status":"proposed","items":[
        {"id":"prove-reopen","title":"Prove reopening","acceptance":"Saved answer survives reopening",
        "priority":"P1","owner":"Michael","due_at_ms":1791800000000i64,"evidence_ids":[]}]});
    let policy=store::open_store(root.path())?;
    policy.execute_batch(super::super::super::jour_fixe_preparation::SCHEMA)?;
    policy.execute("INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-1','project','owner',1791450000000,?1,NULL)",[meeting.to_string()])?;
    let accepted=crate::business_os::command_plane::accept_rxdb_business_command(root.path(),json!({
        "id":"confirm-goal","module":"ctox","record_id":"project","command_type":"ctox.workjet.jour_fixe.todos.confirm",
        "payload":{"meeting_id":"meeting-1","operation_id":"confirm-op","expected_revision":0,
            "proposal_revision":1,"expected_goal_revision":0},
        "client_context":{"actor":{"id":"owner","role":"chef","is_admin":true}}}))?;
    anyhow::ensure!(accepted["status"]=="completed","actual confirmation failed: {accepted}");
    let goal=accepted["result"]["goal"]["goal_id"].as_str().context("actual goal missing")?;
    let emitted=crate::mission::plan::emit_next_step_for_goal(root.path(),goal)?.context("actual step missing")?;
    let core=Connection::open(crate::paths::core_db(root.path()))?;
    assert_eq!(core.execute("UPDATE communication_routing_state SET route_status='leased',lease_owner='fixture-service',
        lease_worker_id='fixture-plan-worker',leased_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
        lease_expires_at=strftime('%Y-%m-%dT%H:%M:%fZ','now','+30 minutes') WHERE message_key=?1",[&emitted.message_key])?,1);
    Ok((root,emitted.message_key))
}
fn token(root:&Path,task:&str)->anyhow::Result<String> {
    issue(root,task,"fixture-plan-worker","native-plan-workspace")?.context("actual plan denied")
}
fn call(root:&Path,trusted:&Value,tool:&str,args:Value)->anyhow::Result<Value> {
    super::super::call_tool_inner(root,tool,args,Some(trusted))
}
fn read()->Value {json!({"action":"read_meeting","request":{"project_id":"project","meeting_id":"meeting-1"}})}
#[test]
fn actual_confirmed_plan_reads_the_goal_and_dispatches_once_without_a_fake_command()->anyhow::Result<()> {
    let (root,task)=fixture()?;
    let registration=call(root.path(),&json!({"auth_source":"ctox_dev_managed_mcp_token","channel":"ctox_dev_managed_mcp",
        "surface":"workjet","actor":"owner","role":"chef","workspace":"tenant:source-owner","instance_id":"source-instance"}),
        workjet_worker_dispatch::TOOL,json!({"action":"register_source","source_environment_id":"source-env",
        "source_supervisor_thread_id":THREAD,"project_id":"project"}))?;
    assert_eq!(registration["state"],"active");
    let trusted=verify_internal_command_session_token(root.path(),&token(root.path(),&task)?)?;
    assert_eq!(trusted["command_id"],"");assert_eq!(trusted["payload_hash"],"");
    assert_eq!(trusted["workjet_confirmed_plan"]["lease"]["task_id"],task);
    let result=call(root.path(),&trusted,workjet_jour_fixe::READ_TOOL,read())?;
    assert_eq!(result["meeting"]["state"],"confirmed");
    assert_eq!(result["meeting"]["todos"]["goal"]["revision"],1);
    let args=json!({"action":"dispatch","dispatch_key":"confirmed-todo-one","task":"Prove reopening"});
    let first=call(root.path(),&trusted,workjet_worker_dispatch::TOOL,args.clone())?;
    assert_eq!(call(root.path(),&trusted,workjet_worker_dispatch::TOOL,args)?,first);
    let core=Connection::open(crate::paths::core_db(root.path()))?;
    assert_eq!(core.query_row("SELECT command_id FROM workjet_worker_dispatch_intents",[],|r|r.get::<_,String>(0))?,task);
    assert_eq!(core.query_row("SELECT count(*) FROM business_command_aggregates WHERE command_id=?1",[&task],|r|r.get::<_,i64>(0))?,0);
    assert!(!crew_only_session_allows_tool("business_os.execute_action",Some(&trusted)));
    assert!(call(root.path(),&trusted,"business_os.execute_action",json!({})).is_err());
    Ok(())
}
#[test]
fn confirmed_plan_can_prepare_the_next_meeting_but_cannot_confirm_owner_todos()->anyhow::Result<()> {
    let (root,task)=fixture()?;
    let trusted=verify_internal_command_session_token(root.path(),&token(root.path(),&task)?)?;
    let policy=store::open_store(root.path())?;
    let raw:String=policy.query_row("SELECT metadata_json FROM workjet_jour_fixe_meetings WHERE meeting_id='meeting-1'",[],|r|r.get(0))?;
    let mut next:Value=serde_json::from_str(&raw)?;
    next["id"]=json!("meeting-next");next["scheduled_at_ms"]=json!(1792054800000i64);
    next["state"]=json!("planned");next["deck_revision"]=json!(0);next["slides"]=json!([]);next["todos"]=Value::Null;
    policy.execute("INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-next','project','owner',1792054800000,?1,NULL)",[next.to_string()])?;
    let args=json!({"action":"prepare_deck","request":{"meeting_id":"meeting-next","operation_id":"next-deck",
        "expected_revision":0,"deck_revision":1,"slides":[{"id":"next-slide","position":0,"title":"Progress",
        "body_markdown":"Actual confirmed task progress","meeting_id":"meeting-next"}]}});
    let first=call(root.path(),&trusted,workjet_jour_fixe::WRITE_TOOL,args.clone())?;
    assert_eq!(first["mutation"]["state"],"preparing");
    assert_eq!(call(root.path(),&trusted,workjet_jour_fixe::WRITE_TOOL,args)?,first);
    assert_eq!(policy.query_row("SELECT command_id FROM workjet_jour_fixe_supervisor_operations WHERE operation_id='next-deck'",[],|r|r.get::<_,String>(0))?,task);
    assert!(call(root.path(),&trusted,workjet_jour_fixe::WRITE_TOOL,json!({"action":"confirm_todos","request":{}})).is_err());
    Ok(())
}

#[test]
fn confirmed_plan_revokes_on_lease_step_goal_source_or_authority_change()->anyhow::Result<()> {
    for change in ["worker","owner","leased_at","expiry","cancel","completed_step","superseded_goal",
        "source","instruction","message","definition","role","epoch","project_owner","binding"] {
        let (root,task)=fixture()?;let token=token(root.path(),&task)?;
        let trusted=verify_internal_command_session_token(root.path(),&token)?;
        let core=Connection::open(crate::paths::core_db(root.path()))?;
        let policy=store::open_store(root.path())?;
        match change {
            "worker"=>{core.execute("UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE message_key=?1",[&task])?;},
            "owner"=>{core.execute("UPDATE communication_routing_state SET lease_owner='other-service' WHERE message_key=?1",[&task])?;},
            "leased_at"=>{core.execute("UPDATE communication_routing_state SET leased_at='2000-01-01T00:00:00Z' WHERE message_key=?1",[&task])?;},
            "expiry"=>{core.execute("UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE message_key=?1",[&task])?;},
            "cancel"=>{core.execute("UPDATE communication_routing_state SET route_status='cancelled' WHERE message_key=?1",[&task])?;},
            "completed_step"=>{core.execute("UPDATE planned_steps SET status='completed'",[])?;},
            "superseded_goal"=>{core.execute("UPDATE planned_goals SET status='superseded'",[])?;},
            "source"=>{core.execute("UPDATE planned_goals SET source_prompt='{}'",[])?;},
            "instruction"=>{core.execute("UPDATE planned_steps SET instruction='Different task'",[])?;},
            "message"=>{core.execute("UPDATE communication_messages SET body_text='Changed prompt' WHERE message_key=?1",[&task])?;},
            "definition"=>{core.execute("UPDATE workjet_project_goal_definitions SET revision=revision+1",[])?;},
            "role"=>{policy.execute("UPDATE business_users SET role='user' WHERE user_id='owner'",[])?;},
            "epoch"=>{policy.execute("UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id='owner'",[])?;},
            "project_owner"=>{let mut row=store::outbound_load_record(&policy,"workjet_projects","project")?.unwrap();row["owner_user_id"]=json!("foreign");
                store::upsert_business_record(&policy,"workjet_projects","project",2,row)?;},
            "binding"=>{policy.execute("UPDATE workjet_supervisor_bindings SET thread_id='foreign-thread' WHERE project_id='project'",[])?;},
            _=>unreachable!(),
        }
        assert!(verify_internal_command_session_token(root.path(),&token).is_err(),"{change}");
        assert!(call(root.path(),&trusted,workjet_jour_fixe::READ_TOOL,read()).is_err(),"{change}");
    }
    Ok(())
}
#[test]
fn ordinary_plan_forged_flags_wrong_worker_and_mixed_grants_never_mint_authority()->anyhow::Result<()> {
    let (root,task)=fixture()?;
    assert!(issue(root.path(),&task,"foreign-worker","workspace").is_err());
    assert!(issue(root.path(),&task,"","workspace").is_err());
    assert!(issue(root.path(),"plan:system::foreign::step","fixture-plan-worker","workspace")?.is_none());
    let token=token(root.path(),&task)?;
    let mut claims=decode_internal_command_session_token(root.path(),&token)?;
    claims.allowed_collections.push("business_commands".into());
    assert!(verify_internal_command_session_token(root.path(),&sign_internal_command_session_claims(root.path(),&claims)?).is_err());
    let forged=json!({"auth_source":"ctox_dev_managed_mcp_token","actor":"owner","role":"chef",
        "workjet_supervisor_only":true,"workjet_confirmed_plan":claims.workjet_confirmed_plan});
    assert!(call(root.path(),&forged,workjet_jour_fixe::READ_TOOL,read()).is_err());
    let core=Connection::open(crate::paths::core_db(root.path()))?;
    core.execute("DELETE FROM workjet_jour_fixe_confirmations",[])?;
    assert!(issue(root.path(),&task,"fixture-plan-worker","workspace")?.is_none());
    Ok(())
}
#[test]
fn native_lease_renewal_keeps_confirmed_plan_session_and_reads_need_no_writer()->anyhow::Result<()> {
    let (root,task)=fixture()?;let token=token(root.path(),&task)?;
    let trusted=verify_internal_command_session_token(root.path(),&token)?;
    let mut core=Connection::open(crate::paths::core_db(root.path()))?;
    core.execute("UPDATE communication_routing_state SET lease_expires_at=strftime('%Y-%m-%dT%H:%M:%fZ','now','+1 hour') WHERE message_key=?1",[&task])?;
    let mut policy=store::open_store(root.path())?;
    let _core_writer=core.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let _policy_writer=policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    assert!(verify_internal_command_session_token(root.path(),&token).is_ok());
    let context=context_from_arguments_with_trusted_gateway_context(workjet_jour_fixe::READ_TOOL,&read(),Some(&trusted))?;
    assert!(workjet_jour_fixe::execute(root.path(),&context,workjet_jour_fixe::READ_TOOL,&read(),Some(&trusted)).is_ok());
    Ok(())
}
