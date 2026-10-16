//! Native P8 + committed manifest/field-mask contract. The bridge only binds
//! an authenticated test participant to the real RightsSnapshot in this TX.
//! It is not a substitute for the still-unbound CUTOVER browser adapter.
use super::*;
use ctox_store::{change_log::subscription::{protocol::{OrderedWrite,Write,WriteStatus},write_contract,WriteAuthority,WritePermission},schema};
use rusqlite::{Connection,Transaction};
use serde_json::{json,Value};
// Fixture marshalling only (RFC7396), not authorization. Both admission and
// final-result policy decisions below use the production manifest/mask code.
fn fixture_delta(target:&mut Value,patch:&Value){
    if let Some(fields)=patch.as_object(){
        if !target.is_object(){*target=json!({});}
        let object=target.as_object_mut().unwrap();
        for (key,value) in fields {if value.is_null(){object.remove(key);}else{fixture_delta(object.entry(key.clone()).or_insert(Value::Null),value);}}
    }else{*target=patch.clone();}
}
struct CommittedPolicy;
impl WriteAuthority for CommittedPolicy {
    fn authorize(&self,tx:&Transaction<'_>,participant:&str,w:&Write)->rusqlite::Result<WritePermission>{
        let r=read(tx)?;
        let allowed=r.resolve_app_collection("p6-leads","user",participant,&w.collection)
            .ok().is_some_and(|s|s.projection(&w.collection).is_some());
        if !allowed{return Ok(WritePermission::Reject("forbidden".into()));}
        // Guard the actual incoming delta before a three-way merge may erase
        // its forbidden intent; the native browser binding is still pending.
        let c=schema::active_collection(tx,&w.collection)?.unwrap();
        let raw:String=tx.query_row(&format!("SELECT data FROM {} WHERE id=?1",c.table),[&w.id],|r|r.get(0))?;
        let before:Value=serde_json::from_str(&raw).unwrap();let mut candidate=before.clone();
        fixture_delta(&mut candidate,w.body.as_ref().unwrap());
        let actor=crate::business_os::policy::BusinessOsActor::new(Some(participant.into()),"user");
        Ok(match crate::business_os::policy::write_sync_document(&actor,&r,"p6-leads","leads",Some(&before),Some(&candidate),None){
            Ok(())=>WritePermission::Permit,Err(reason)=>WritePermission::Reject(reason)
        })
    }
    fn validate_document(&self,tx:&Transaction<'_>,participant:&str,c:&schema::Collection,doc:&Value)->rusqlite::Result<WritePermission>{
        let raw:String=tx.query_row(&format!("SELECT data FROM {} WHERE id=?1",c.table),[doc["id"].as_str().unwrap()],|r|r.get(0))?;
        let before:Value=serde_json::from_str(&raw).unwrap();
        let actor=crate::business_os::policy::BusinessOsActor::new(Some(participant.into()),"user");
        Ok(match crate::business_os::policy::write_sync_document(&actor,&read(tx)?,"p6-leads","leads",Some(&before),Some(doc),None){
            Ok(())=>WritePermission::Permit,Err(reason)=>WritePermission::Reject(reason)
        })
    }
}
fn check(case:&str){
    let input:Value=serde_json::from_slice(&std::fs::read(std::env::var("P6_FIELDS_INPUT").unwrap()).unwrap()).unwrap();
    assert_eq!(input["case"],case);
    let w:OrderedWrite=serde_json::from_value(input["wire"].clone()).unwrap();
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("fields.sqlite");
    let mut db=Connection::open(&path).unwrap();
    ctox_store::config::ConnectionConfig::default().apply_writer(&db).unwrap();
    let tx=db.transaction().unwrap();
    schema::create_collection(&tx,&schema::CollectionDefinition{
        name:"leads".into(),app_id:"p6-leads".into(),schema_ver:1,
        schema_json:json!({"type":"object","properties":{"id":{"type":"string"},"native_epoch":{"type":"integer"},"pipeline":{"type":"object"}}}),
        class:"replicated".into(),client_limit:100,byte_budget:1048576,rights_ver:1,app_ver:"1".into()
    }).unwrap();
    initialize(&tx).unwrap();
    let manifest=manifest_rules::ManifestRules::from_manifest("p6-leads","1",["leads".into()].into(),&json!({
        "write_guard":{"leads":{"command_only_fields":["native_epoch","/pipeline/native_score"]}}
    })).unwrap();
    replace_with_manifests(&tx,1,&[Right{role:"user".into(),collection:"leads".into(),row_filter:scope::RowFilter::All,
        field_mask:scope::FieldMask::Fields{paths:["/pipeline/stage".into()].into()},ops:[Operation::Read,Operation::Write].into()}],&[manifest]).unwrap();
    let c=schema::active_collection(&tx,"leads").unwrap().unwrap();
    let old=json!({"id":"lead","native_epoch":1,"pipeline":{"stage":"open","native_score":10}});
    let new=json!({"id":"lead","native_epoch":2,"pipeline":{"stage":"open","native_score":20}});
    tx.execute(&format!("INSERT INTO {}(id,rev,schema_ver,data,deleted) VALUES('lead','native:old',1,?1,0)",c.table),[old.to_string()]).unwrap();
    tx.execute("INSERT INTO _revs(collection,id,rev,seq,gen,body,created_at) SELECT 'leads','lead','native:old',seq,1,?1,?2 FROM _changes WHERE collection='leads' AND id='lead'",rusqlite::params![old.to_string(),now/1000]).unwrap();
    tx.execute(&format!("UPDATE {} SET rev='native:new',data=?1 WHERE id='lead'",c.table),[new.to_string()]).unwrap();
    tx.commit().unwrap();
    let tx=db.transaction().unwrap();
    let batch=write_contract::submit_in_transaction(&tx,"alice",&w,now,&CommittedPolicy).unwrap();
    assert_eq!(batch.results.len(),1);let receipt=serde_json::to_value(&batch.results[0]).unwrap();
    tx.commit().unwrap();drop(db);
    // Reopen the file: targeted denial and both journals must survive the TX.
    let db=Connection::open(&path).unwrap();
    let body:String=db.query_row(&format!("SELECT data FROM {} WHERE id='lead'",c.table),[],|r|r.get(0)).unwrap();
    let final_doc:Value=serde_json::from_str(&body).unwrap();
    let ops:Option<String>=db.query_row("SELECT result_json FROM _ops WHERE op_id=?1",[&w.write.op_id],|r|r.get(0)).optional().unwrap();
    let ordered:String=db.query_row("SELECT result_json FROM _write_results WHERE participant='alice' AND client_seq=1",[],|r|r.get(0)).unwrap();
    let report=json!({"case":case,"receipt":receipt,"doc":final_doc,"ops":ops.as_ref().map(|s|serde_json::from_str::<Value>(s).unwrap()),"ordered":serde_json::from_str::<Value>(&ordered).unwrap(),"journalReopened":true});
    std::fs::write(std::env::var("P6_FIELDS_OUTPUT").unwrap(),serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    assert_eq!(final_doc["native_epoch"],2,"native epoch changed");assert_eq!(final_doc["pipeline"]["native_score"],20,"nested native field changed");
    if case=="patch" {
        assert_ne!(batch.results[0].result.status,WriteStatus::Rejected,"peer-only patch was rejected after concurrent native update");
        assert_eq!(final_doc["pipeline"]["stage"],"won");
    } else {
        assert_eq!(batch.results[0].result.status,WriteStatus::Rejected,"server-owned intent was accepted after merge erased it");
        assert_eq!(batch.results[0].result.reason.as_deref(),Some("command_only_field"),"server-owned mutation lost specific rejection");
        assert_eq!(final_doc,new,"denied server-owned write changed the document");
        assert!(ops.is_some(),"server-owned rejection missing from _ops");
        assert_eq!(report["ops"]["reason"],"command_only_field");assert_eq!(report["ordered"]["reason"],"command_only_field");
    }
}
#[test] fn p6_fields_peer_patch_preserves_concurrent_native_fields(){check("patch");}
#[test] fn p6_fields_explicit_server_change_rejected_and_journaled(){check("guard");}
#[test] fn p6_fields_stale_server_intent_rejected_and_journaled(){check("stale");}
