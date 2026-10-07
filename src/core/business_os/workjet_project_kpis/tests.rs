// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::business_os::{command_plane, store, store_workjet_projects};
use tempfile::{tempdir, TempDir};
const OWNER: &str = "196a89ba-ee86-4413-885c-04ca60e6f291";
const ALIAS: &str = "michael.welsch@metric-space.ai";
const FOREIGN: &str = "foreign@example.org";

fn fixture() -> anyhow::Result<TempDir> {
    let root = tempdir()?;
    drop(crate::business_os::store_projections::tests::create_repair_rxdb_tables(root.path())?);
    store_workjet_projects::tests::create_workjet_rxdb_projection_tables(root.path())?;
    let now = store::now_ms() as i64;
    let _ = store::issue_business_os_capability_token_for_managed_user_with_email(
        root.path(),OWNER,Some(ALIAS),"Michael","chef",now)?;
    for user in [ALIAS, FOREIGN] {
        let _ = store::issue_business_os_capability_token_for_managed_user(
            root.path(),user,"Michael","admin",now)?;
    }
    store::upsert_business_record(&store::open_store(root.path())?,"workjet_projects","project-1",1,
        json!({"id":"project-1","name":"Project","owner_user_id":OWNER,"status":"active",
            "repo_url":"https://github.com/metric-space-ai/ctox","public_url":"https://ctox.dev",
            "info":{"goal":"Keep this"},"jour_fixe":{"weekday":1,"time":"13:00","timezone":"Europe/Berlin"},
            "created_at_ms":1,"updated_at_ms":1,"is_deleted":false}))?;
    Ok(root)
}
fn send(root: &Path, actor: &str, id: &str, kind: &str, payload: Value) -> anyhow::Result<Value> {
    command_plane::accept_rxdb_business_command(root,
        json!({"id":id,"module":"ctox","command_type":kind,"record_id":"project-1",
            "payload":payload,"client_context":{"actor":{"id":actor,"role":"admin"}}}))
}
fn configuration(op: &str, revision: u64, prompts: Value) -> Value {
    json!({"operation_id":op,"project_id":"project-1","expected_revision":revision,"prompts":prompts})
}
fn configure(root: &Path, id: &str, revision: u64, prompts: Value) -> anyhow::Result<Value> {
    send(root,ALIAS,id,"ctox.workjet.project.kpis.configure",configuration(id,revision,prompts))
}
fn read(root: &Path, actor: &str, id: &str) -> anyhow::Result<Value> {
    send(root,actor,id,"ctox.workjet.project.kpis.read",json!({"project_id":"project-1"}))
}
fn rejected(result: anyhow::Result<Value>) {
    assert!(result.is_err() || result.as_ref().is_ok_and(|v|v["status"]=="failed"),"{result:?}");
}

#[test]
fn owner_alias_persists_prompts_and_no_numbers_without_rewriting_project() -> anyhow::Result<()> {
    let root = fixture()?;
    let before = store::outbound_load_record(&store::open_store(root.path())?,"workjet_projects","project-1")?;
    let prompts = json!([{"kpi_id":"users","prompt":"Weekly active users"},
        {"kpi_id":"sales","prompt":"Revenue this month"},{"kpi_id":"prs","prompt":"Merged PRs this week"}]);
    let saved = configure(root.path(),"save",0,prompts)?;
    assert_eq!(saved["status"],"completed");
    let state = &saved["result"]["kpis"];
    assert_eq!(state["revision"],1);
    assert_eq!(state["items"].as_array().context("items")?.len(),3);
    for item in state["items"].as_array().unwrap() {
        assert_eq!(item["prompt"]["revision"],1);
        assert_eq!(item["result"]["status"],"missing_source");
        assert!(item["result"].get("snapshot").is_none());
    }
    assert_eq!(read(root.path(),OWNER,"read-owner")?["result"]["kpis"],*state);
    assert_eq!(read(root.path(),ALIAS,"read-alias")?["result"]["kpis"],*state);
    assert_eq!(store::outbound_load_record(&store::open_store(root.path())?,"workjet_projects","project-1")?,before);
    Ok(())
}

#[test]
fn empty_project_has_revision_zero_without_creating_native_state() -> anyhow::Result<()> {
    let root=fixture()?;
    assert_eq!(read(root.path(),ALIAS,"empty")?["result"]["kpis"],
        json!({"project_id":"project-1","revision":0,"items":[]}));
    let exists:bool=store::open_store(root.path())?.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_project_kpi_state')",[],|r|r.get(0))?;
    assert!(!exists);
    Ok(())
}

#[test]
fn operation_replay_is_idempotent_and_conflicting_intent_does_not_mutate() -> anyhow::Result<()> {
    let root=fixture()?;
    let payload=configuration("same-operation",0,json!([{"kpi_id":"k","prompt":"Active users"}]));
    let first=send(root.path(),ALIAS,"first-command","ctox.workjet.project.kpis.configure",payload.clone())?;
    let replay=send(root.path(),ALIAS,"second-command","ctox.workjet.project.kpis.configure",payload.clone())?;
    assert_eq!(replay["status"],"completed");
    assert_eq!(replay["result"],first["result"]);
    let mut conflict=payload;
    conflict["prompts"][0]["prompt"]=json!("A different KPI");
    rejected(send(root.path(),ALIAS,"conflicting-intent","ctox.workjet.project.kpis.configure",conflict));
    assert_eq!(read(root.path(),OWNER,"read")?["result"],first["result"]);
    Ok(())
}

#[test]
fn stale_expected_revision_fails_without_clearing_current_prompts() -> anyhow::Result<()> {
    let root=fixture()?;
    let first=configure(root.path(),"first",0,json!([{"kpi_id":"k","prompt":"Active users"}]))?;
    rejected(configure(root.path(),"stale",0,json!([])));
    assert_eq!(read(root.path(),OWNER,"current")?["result"],first["result"]);
    Ok(())
}

#[test]
fn prompt_revisions_survive_clear_and_never_reuse_a_removed_identity() -> anyhow::Result<()> {
    let root=fixture()?;
    let p=json!([{"kpi_id":"k","prompt":"Active users"}]);
    configure(root.path(),"first",0,p.clone())?;
    let unchanged=configure(root.path(),"unchanged",1,p)?;
    assert_eq!(unchanged["result"]["kpis"]["items"][0]["prompt"]["revision"],1);
    let changed=configure(root.path(),"changed",2,json!([{"kpi_id":"k","prompt":"Weekly active users"}]))?;
    assert_eq!(changed["result"]["kpis"]["items"][0]["prompt"]["revision"],2);
    let cleared=configure(root.path(),"clear",3,json!([]))?;
    assert_eq!(cleared["result"]["kpis"]["items"],json!([]));
    let restored=configure(root.path(),"restore",4,json!([{"kpi_id":"k","prompt":"Active users"}]))?;
    assert_eq!(restored["result"]["kpis"]["items"][0]["prompt"]["revision"],3);
    assert_eq!(restored["result"]["kpis"]["revision"],5);
    Ok(())
}

#[test]
fn foreign_actor_and_foreign_signed_peer_cannot_read_or_configure() -> anyhow::Result<()> {
    let root=fixture()?;
    rejected(read(root.path(),FOREIGN,"foreign-read"));
    rejected(send(root.path(),FOREIGN,"foreign-save","ctox.workjet.project.kpis.configure",
        configuration("foreign",0,json!([]))));
    let (token,_)=store::issue_business_os_capability_token_for_managed_user(root.path(),FOREIGN,
        "Michael","admin",store::now_ms() as i64)?;
    rejected(store::accept_rxdb_business_command_with_origin(root.path(),json!({
        "id":"signed-foreign","module":"ctox","command_type":"ctox.workjet.project.kpis.read",
        "record_id":"project-1","payload":{"project_id":"project-1"},
        "client_context":{"actor":{"id":ALIAS,"role":"chef","email":ALIAS},"capability_token":token}}),
        store::CommandOrigin::ReplicatedPeer));
    Ok(())
}

#[test]
fn revoked_alias_and_archived_or_deleted_project_are_not_readable() -> anyhow::Result<()> {
    for reason in ["revoked-alias","revoked-owner","archived","deleted"] {
        let root=fixture()?;
        configure(root.path(),"first",0,json!([{"kpi_id":"k","prompt":"Users"}]))?;
        let conn=store::open_store(root.path())?;
        if reason.starts_with("revoked") {
            let id=if reason=="revoked-alias" {ALIAS} else {OWNER};
            conn.execute("UPDATE business_users SET active=0 WHERE user_id=?1",[id])?;
        } else {
            let mut project=store::outbound_load_record(&conn,"workjet_projects","project-1")?.context("project")?;
            if reason=="archived" {project["status"]=json!("archived");}
            else {project["_deleted"]=json!(true);}
            store::upsert_business_record(&conn,"workjet_projects","project-1",2,project)?;
        }
        drop(conn);
        rejected(read(root.path(),ALIAS,"after-revocation"));
        rejected(configure(root.path(),"revoked-edit",1,json!([])));
    }
    Ok(())
}

#[test]
fn invalid_prompts_and_caller_supplied_values_cannot_become_a_snapshot() -> anyhow::Result<()> {
    let root=fixture()?;
    let invalid=vec![
        json!([{"kpi_id":"k","prompt":"   "}]),
        json!([{"kpi_id":"k","prompt":"Users","value":999}]),
        json!([{"kpi_id":"k","prompt":"Users"},{"kpi_id":"k","prompt":"Sales"}]),
        json!([{"kpi_id":"1","prompt":"A"},{"kpi_id":"2","prompt":"B"},
            {"kpi_id":"3","prompt":"C"},{"kpi_id":"4","prompt":"D"}]),
    ];
    for (i,prompts) in invalid.into_iter().enumerate() {
        rejected(configure(root.path(),&format!("invalid-{i}"),0,prompts));
    }
    let mut forged=configuration("forged-result",0,json!([]));
    forged["result"]=json!({"status":"ready","value":999});
    rejected(send(root.path(),ALIAS,"forged-result","ctox.workjet.project.kpis.configure",forged));
    assert_eq!(read(root.path(),OWNER,"read-after-invalid")?["result"]["kpis"]["revision"],0);
    Ok(())
}

#[test]
fn failed_state_write_rolls_back_prompt_watermark_and_domain_receipt() -> anyhow::Result<()> {
    let root=fixture()?;
    let first=configure(root.path(),"first",0,json!([{"kpi_id":"k","prompt":"Users"}]))?;
    let conn=store::open_store(root.path())?;
    conn.execute_batch("CREATE TRIGGER reject_kpi_update BEFORE UPDATE ON workjet_project_kpi_state
        BEGIN SELECT RAISE(ABORT,'test state write failure'); END;")?;
    drop(conn);
    let updated=json!([{"kpi_id":"k","prompt":"Weekly users"}]);
    rejected(configure(root.path(),"failed-write",1,updated.clone()));
    assert_eq!(read(root.path(),OWNER,"unchanged")?["result"],first["result"]);
    let conn=store::open_store(root.path())?;
    let receipts:i64=conn.query_row("SELECT COUNT(*) FROM business_command_domain_effects WHERE command_id='failed-write'",[],|r|r.get(0))?;
    assert_eq!(receipts,0);
    conn.execute_batch("DROP TRIGGER reject_kpi_update")?;
    drop(conn);
    let saved=configure(root.path(),"recovery",1,updated)?;
    assert_eq!(saved["status"],"completed");
    assert_eq!(saved["result"]["kpis"]["revision"],2);
    assert_eq!(saved["result"]["kpis"]["items"][0]["prompt"]["revision"],2);
    Ok(())
}
