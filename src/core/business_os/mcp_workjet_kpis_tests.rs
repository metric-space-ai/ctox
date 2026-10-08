// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
const THREAD: &str="cc6cfe73-2824-4360-9daf-3b3efb079931";
fn fixture() -> anyhow::Result<(tempfile::TempDir,Value)> {
    let (root,trusted)=workjet_worker_dispatch::meeting_test_fixture()?;
    configure(root.path(),"config",0,"Completed project tasks in the last seven days")?;
    Ok((root,trusted))
}
fn configure(root: &Path, id: &str, revision: u64, prompt: &str) -> anyhow::Result<Value> {
    let result=super::super::super::command_plane::accept_rxdb_business_command(root,json!({"id":id,"module":"ctox","record_id":"project","command_type":"ctox.workjet.project.kpis.configure",
      "payload":{"project_id":"project","operation_id":id,"expected_revision":revision,"prompts":[{"kpi_id":"k","prompt":prompt}]},
      "client_context":{"actor":{"id":"owner","role":"chef"}}}))?;
    anyhow::ensure!(result["status"]=="completed","{result}");
    Ok(result)
}
fn call(root: &Path, trusted: &Value, args: Value) -> anyhow::Result<Value> {
    super::super::call_tool_inner(root,TOOL,args,Some(trusted))
}
fn args(recipe: &str, operation: &str, revision: u64) -> Value {
    json!({"action":"resolve","request":{"operation_id":operation,"project_id":"project","kpi_id":"k","prompt_revision":1,"expected_revision":revision,"recipe":recipe,"window_days":7}})
}
fn read(root: &Path,trusted: &Value) -> anyhow::Result<Value> {call(root,trusted,json!({"action":"read","request":{"project_id":"project"}}))}
fn core(root: &Path) -> anyhow::Result<Connection> {Ok(Connection::open(crate::paths::core_db(root))?)}
fn add(root: &Path,trusted: &Value,id: &str,phase: &str,status: &str,owner: &str,project: &str,thread: &str,created: i64) -> anyhow::Result<()> {
    let command=required_arg(trusted,"command_id")?;
    core(root)?.execute("INSERT INTO business_command_aggregates
      (command_id,idempotency_key,payload_hash,module,command_type,record_id,execution_mode,execution_phase,terminal_status,intent_json,created_at_ms,updated_at_ms)
      SELECT ?1,?1,payload_hash,module,command_type,?6,execution_mode,?2,?3,
      json_set(intent_json,'$.client_context.actor.id',?4,'$.payload.thread_id',?5,'$.record_id',?6),?7,?7
      FROM business_command_aggregates WHERE command_id=?8",
      rusqlite::params![id,phase,status,owner,thread,project,created,command])?;
    Ok(())
}
#[test]
fn native_recipe_calculates_real_scoped_receipts_and_retains_definition() -> anyhow::Result<()> {
    let (root,trusted)=fixture()?;
    let now=store::now_ms() as i64;
    add(root.path(),&trusted,"done","terminal","completed","owner","project",THREAD,now)?;
    for (id,owner,project,thread,created) in [
      ("foreign-owner","foreign","project",THREAD,now),("foreign-project","owner","other",THREAD,now),
      ("foreign-thread","owner","project","other-thread",now),("old","owner","project",THREAD,now-8*24*3600000)] {
      add(root.path(),&trusted,id,"terminal","completed",owner,project,thread,created)?;
    }
    let result=call(root.path(),&trusted,args("project_tasks_completed","resolve",1))?;
    let snapshot=&result["kpis"]["items"][0]["result"]["snapshot"];
    assert_eq!(snapshot["value"],1);assert_eq!(snapshot["prompt_revision"],1);
    assert_eq!(snapshot["sources"][0]["project_id"],"project");
    assert!(snapshot["sources"][0]["evidence_ref"].as_str().unwrap().starts_with("native-project-task-snapshot:"));
    assert!(snapshot["label"].as_str().unwrap().chars().count()<=14);
    let policy=store::open_store(root.path())?;
    let raw:String=policy.query_row("SELECT request_json FROM workjet_project_kpi_definitions WHERE project_id='project'",[],|r|r.get(0))?;
    assert_eq!(serde_json::from_str::<Value>(&raw)?["recipe"],"project_tasks_completed");
    assert_eq!(call(root.path(),&trusted,args("project_tasks_completed","resolve",1))?,result);
    assert_eq!(read(root.path(),&trusted)?["kpis"],result["kpis"]);
    Ok(())
}
#[test]
fn missing_github_and_empty_denominator_never_become_invented_zero() -> anyhow::Result<()> {
    for recipe in ["github_merged_prs","project_tasks_success_rate"] {
      let (root,trusted)=fixture()?;
      let result=call(root.path(),&trusted,args(recipe,"missing",1))?;
      assert_eq!(result["kpis"]["items"][0]["result"]["status"],"missing_source");
      assert!(result["kpis"]["items"][0]["result"].get("snapshot").is_none());
    }
    Ok(())
}
#[test]
fn refresh_is_hourly_and_preparation_can_force_a_fresh_real_snapshot() -> anyhow::Result<()> {
    let (root,trusted)=fixture()?;
    let first=call(root.path(),&trusted,args("project_tasks_completed","first",1))?;
    let at=first["kpis"]["items"][0]["result"]["snapshot"]["freshness"]["calculated_at_ms"].as_i64().unwrap();
    add(root.path(),&trusted,"completed-later","terminal","completed","owner","project",THREAD,at+10)?;
    resolver::refresh_test_at(root.path(),None,false,at+100)?;
    assert_eq!(read(root.path(),&trusted)?["kpis"]["revision"],2);
    resolver::refresh_test_at(root.path(),Some("project"),true,at+100)?;
    let second=read(root.path(),&trusted)?;
    assert_eq!(second["kpis"]["revision"],3);assert_eq!(second["kpis"]["items"][0]["result"]["snapshot"]["value"],1);
    resolver::refresh_test_at(root.path(),None,false,at+3600000+100)?;
    assert_eq!(read(root.path(),&trusted)?["kpis"]["revision"],4);
    Ok(())
}
#[test]
fn changed_or_cleared_prompt_retires_recipe_and_rejects_late_resolution() -> anyhow::Result<()> {
    let (root,trusted)=fixture()?;
    call(root.path(),&trusted,args("project_tasks_completed","old-bind",1))?;
    configure(root.path(),"new-prompt",2,"Active visitors this week")?;
    assert!(call(root.path(),&trusted,args("project_tasks_completed","old-bind",1)).is_err());
    resolver::refresh_test_at(root.path(),None,true,store::now_ms() as i64+3600000)?;
    let result=read(root.path(),&trusted)?;
    assert_eq!(result["kpis"]["revision"],3);assert_eq!(result["kpis"]["items"][0]["prompt"]["revision"],2);
    assert_eq!(result["kpis"]["items"][0]["result"]["status"],"missing_source");
    assert_eq!(store::open_store(root.path())?.query_row("SELECT count(*) FROM workjet_project_kpi_definitions",[],|r|r.get::<_,u64>(0))?,0);
    Ok(())
}
#[test]
fn revoked_or_rebound_authority_cannot_refresh_or_replay() -> anyhow::Result<()> {
    for change in ["revoked","rebound","archive","lease"] {
      let (root,trusted)=fixture()?;
      call(root.path(),&trusted,args("project_tasks_completed","bind",1))?;
      let policy=store::open_store(root.path())?;
      match change {
        "revoked"=>{policy.execute("UPDATE business_users SET active=0 WHERE user_id='owner'",[])?;},
        "rebound"=>{policy.execute("UPDATE workjet_supervisor_bindings SET thread_id='replacement' WHERE project_id='project'",[])?;},
        "archive"=>{let mut project=store::outbound_load_record(&policy,"workjet_projects","project")?.unwrap();project["status"]=json!("archived");store::upsert_business_record(&policy,"workjet_projects","project",2,project)?;},
        "lease"=>{core(root.path())?.execute("UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE route_status='leased'",[])?;},
        _=>unreachable!(),
      }
      assert!(call(root.path(),&trusted,args("project_tasks_completed","bind",1)).is_err(),"{change}");
      if change!="lease" {
        resolver::refresh_test_at(root.path(),None,true,store::now_ms() as i64+3600000)?;
        let raw:String=policy.query_row("SELECT state_json FROM workjet_project_kpi_state",[],|r|r.get(0))?;
        assert_eq!(serde_json::from_str::<Value>(&raw)?["revision"],2,"{change}");
      }
    }
    Ok(())
}
#[test]
fn values_sql_unknown_sources_and_wrong_revisions_are_rejected_without_mutation() -> anyhow::Result<()> {
    for change in ["value","sql","project","prompt","revision","window","recipe"] {
      let (root,trusted)=fixture()?;
      let mut request=args("project_tasks_completed","bad",1);
      match change {
        "value"=>request["request"]["value"]=json!(99),"sql"=>request["request"]["sql"]=json!("SELECT secrets"),
        "project"=>request["request"]["project_id"]=json!("foreign"),"prompt"=>request["request"]["prompt_revision"]=json!(2),
        "revision"=>request["request"]["expected_revision"]=json!(99),"window"=>request["request"]["window_days"]=json!(366),
        "recipe"=>request["request"]["recipe"]=json!("tenant_total"),_=>unreachable!(),
      }
      assert!(call(root.path(),&trusted,request).is_err(),"{change}");
      assert_eq!(read(root.path(),&trusted)?["kpis"]["revision"],1);
    }
    Ok(())
}
#[test]
fn ordinary_owner_foreign_peer_and_changed_lease_have_no_supervisor_metric_authority() -> anyhow::Result<()> {
    let (root,trusted)=fixture()?;
    for actor in ["owner","foreign"] {
      let gateway=json!({"auth_source":"ctox_dev_managed_mcp_token","channel":"ctox_dev_managed_mcp","surface":"workjet","actor":actor,"role":"chef","workspace":"tenant:source-owner","instance_id":"source-instance"});
      assert!(call(root.path(),&gateway,args("project_tasks_completed","forged",1)).is_err());
    }
    core(root.path())?.execute("UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE route_status='leased'",[])?;
    assert!(read(root.path(),&trusted).is_err());
    assert!(call(root.path(),&trusted,args("project_tasks_completed","replaced",1)).is_err());
    Ok(())
}
#[test]
fn expired_snapshot_read_is_stale_and_does_not_take_writer_locks() -> anyhow::Result<()> {
    let (root,trusted)=fixture()?;
    call(root.path(),&trusted,args("project_tasks_completed","first",1))?;
    let mut policy=store::open_store(root.path())?;
    policy.execute("UPDATE workjet_project_kpi_state SET state_json=json_set(state_json,'$.items[0].result.snapshot.freshness.calculated_at_ms',1,'$.items[0].result.snapshot.freshness.refresh_at_ms',2,'$.items[0].result.snapshot.freshness.fresh_until_ms',3,'$.items[0].result.snapshot.sources[0].observed_at_ms',1)",[])?;
    let lock=policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result=read(root.path(),&trusted)?;
    assert_eq!(result["kpis"]["items"][0]["result"]["status"],"stale");
    lock.rollback()?;
    Ok(())
}
