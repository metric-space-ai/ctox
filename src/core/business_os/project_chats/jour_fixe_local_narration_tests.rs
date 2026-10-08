// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::business_os::{command_plane,workjet_jour_fixe_contract as wire};
use base64::Engine;
const KIND:&str="ctox.workjet.jour_fixe.narration.local_publish";
fn sha(v:&[u8])->String {format!("{:x}",Sha256::digest(v))}
fn wav()->Vec<u8> {
 let pcm=vec![1u8;64000]; let mut v=Vec::new();
 v.extend(b"RIFF");v.extend((36u32+pcm.len() as u32).to_le_bytes());v.extend(b"WAVEfmt ");
 v.extend(16u32.to_le_bytes());v.extend(1u16.to_le_bytes());v.extend(1u16.to_le_bytes());
 v.extend(16000u32.to_le_bytes());v.extend(32000u32.to_le_bytes());v.extend(2u16.to_le_bytes());v.extend(16u16.to_le_bytes());
 v.extend(b"data");v.extend((pcm.len() as u32).to_le_bytes());v.extend(pcm);v
}
fn token(root:&Path,actor:&str)->anyhow::Result<String>{Ok(store::issue_business_os_capability_token_for_managed_user(root,actor,actor,"admin",chrono::Utc::now().timestamp_millis())?.0)}
fn send(root:&Path,id:&str,actor:&str,t:&str,p:Value)->anyhow::Result<Value>{
 command_plane::accept_rxdb_business_command_with_origin(root,json!({"id":id,"module":"ctox","record_id":"project","command_type":KIND,"payload":p,"client_context":{"actor":{"id":actor},"capability_token":t}}),CommandOrigin::ReplicatedPeer)
}
fn saved(root:&Path)->anyhow::Result<Value>{super::jour_fixe_owner::saved(root)}
fn change(root:&Path,f:impl FnOnce(&mut Value))->anyhow::Result<()> {
 let mut v=saved(root)?; f(&mut v); open_store(root)?.execute("UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",[v.to_string()])?; Ok(())
}
fn fixture()->anyhow::Result<(TempDir,String,Value)> {
 let root=super::jour_fixe_owner::fixture("preparing")?;
 change(root.path(),|v|{v["slides"][0]["audio"]=Value::Null;v["slides"][0]["body_markdown"]=json!("Native persistence is verified.");})?;
 let db=Connection::open(store::rxdb_store_path(root.path()))?;
 for collection in ["desktop_files","desktop_file_chunks"] {db.execute_batch(&format!("CREATE TABLE IF NOT EXISTS ctox_business_os__{collection}__v0(id TEXT PRIMARY KEY,rev TEXT NOT NULL,deleted INTEGER NOT NULL,last_write_time REAL NOT NULL,data TEXT NOT NULL)"))?;}
 let bytes=wav(); let hash=sha(&bytes); let now=chrono::Utc::now().timestamp_millis();
 let file=json!({"id":"upload","name":"actual.wav","kind":"file","owner_id":"owner","mime_type":"audio/wav","size_bytes":bytes.len(),"content_state":"available","content_generation_id":"upload-gen","content_hash":hash,"content_hash_scheme":"sha256-bytes-v1","created_at_ms":now,"updated_at_ms":now});
 db.execute("INSERT INTO ctox_business_os__desktop_files__v0 VALUES ('upload','1-fixture',0,1,?1)",[file.to_string()])?;
 let encoded=base64::engine::general_purpose::STANDARD.encode(&bytes); let total=encoded.len().div_ceil(16384);
 for (idx,part) in encoded.as_bytes().chunks(16384).enumerate() {
  let data=std::str::from_utf8(part)?;let id=format!("upload_upload-gen_{idx:06}");
  let chunk=json!({"id":id,"file_id":"upload","generation_id":"upload-gen","idx":idx,"total":total,"encoding":"base64","data":data,"chunk_hash":sha(data.as_bytes()),"chunk_hash_scheme":"sha256-base64-chunk-v1","created_at_ms":now});
  db.execute("INSERT INTO ctox_business_os__desktop_file_chunks__v0 VALUES (?1,'1-fixture',0,1,?2)",rusqlite::params![id,chunk.to_string()])?;
 }
 let t=token(root.path(),"owner")?;
 let p=json!({"operation_id":"audio-op","instance_id":store::stable_instance_id(root.path())?,"project_id":"project","meeting_id":"meeting-1","slide_id":"slide-1","file_id":"upload","generation_id":"upload-gen","deck_revision":1,"expected_revision":0,"audio_sha256":hash,"narration_text_sha256":sha(b"Native persistence is verified.")});
 Ok((root,t,p))
}
fn rejected(result:anyhow::Result<Value>){assert!(result.is_err() || result.as_ref().is_ok_and(|v|v["status"]=="failed"||v["ok"]==false),"{result:?}");}
fn copies(root:&Path)->anyhow::Result<i64>{Ok(open_store(root)?.query_row("SELECT count(*) FROM business_records WHERE collection='desktop_files' AND json_extract(payload_json,'$.source')='ctox-jour-fixe-local-audio'",[],|r|r.get(0))?)}
#[test]
fn real_owner_wav_is_copied_with_exact_duration_hash_and_local_provenance()->anyhow::Result<()> {
 let (root,t,p)=fixture()?;let result=send(root.path(),"publish","owner",&t,p.clone())?;
 assert_eq!(result["status"],"completed","{result}");let r=&result["result"]["local_narration"];
 let _:wire::LocalNarrationReceipt=serde_json::from_value(r.clone())?;
 assert_eq!(r["provider_verified"],false);assert_eq!(r["provenance"],"authenticated_owner_local_audio");
 assert_eq!(r["audio"]["sha256"],p["audio_sha256"]);assert_eq!(r["audio"]["duration_ms"],2000);assert_ne!(r["audio"]["file_id"],"upload");
 assert_eq!(r["audio"]["model"],"owner-uploaded-local-audio");assert_eq!(r["audio"]["synthesis_duration_ms"],0);
 assert_eq!(saved(root.path())?["state"],"ready");assert_eq!(copies(root.path())?,1);
 let file_id=r["audio"]["file_id"].as_str().context("native file")?;
 let doc=store::rxdb_desktop_file_document(root.path(),file_id)?;assert_eq!(doc["content_hash"],sha(&wav()));
 let db=Connection::open(store::rxdb_store_path(root.path()))?;
 db.execute("DELETE FROM ctox_business_os__desktop_file_chunks__v0 WHERE json_extract(data,'$.file_id')='upload'",[])?;
 db.execute("DELETE FROM ctox_business_os__desktop_files__v0 WHERE id='upload'",[])?;
 let replay=send(root.path(),"replay","owner",&t,p.clone())?;assert_eq!(replay["result"],result["result"]);
 let recovered=command_plane::recover_applied_domain_effect_for_intake(root.path(),"publish")?.context("native recovery")?;
 assert_eq!(recovered["result"],result["result"]);assert_eq!(saved(root.path())?["revision"],1);assert_eq!(copies(root.path())?,1);
 Ok(())
}
#[test]
fn every_slide_requires_real_audio_before_ready_and_new_operations_do_not_overwrite()->anyhow::Result<()> {
 let (root,t,p)=fixture()?;
 change(root.path(),|v|{let mut slide=v["slides"][0].clone();slide["id"]=json!("slide-2");slide["position"]=json!(1);v["slides"].as_array_mut().unwrap().push(slide);})?;
 let first=send(root.path(),"first","owner",&t,p.clone())?;assert_eq!(first["status"],"completed","{first}");assert_eq!(saved(root.path())?["state"],"preparing");
 let mut q=p.clone();q["operation_id"]=json!("overwriting");q["expected_revision"]=json!(1);
 rejected(send(root.path(),"overwrite","owner",&t,q));assert_eq!(copies(root.path())?,1);
 let mut q=p;q["operation_id"]=json!("second-op");q["slide_id"]=json!("slide-2");q["expected_revision"]=json!(1);
 let second=send(root.path(),"second","owner",&t,q)?;assert_eq!(second["status"],"completed","{second}");
 assert_eq!(saved(root.path())?["state"],"ready");assert_eq!(copies(root.path())?,2);Ok(())
}
#[test]
fn foreign_peer_and_client_provider_owner_duration_claims_are_rejected()->anyhow::Result<()> {
 let (root,t,p)=fixture()?;let foreign=token(root.path(),"foreign")?;
 rejected(send(root.path(),"foreign","foreign",&foreign,p.clone()));
 for (key,value) in [("owner_user_id",json!("owner")),("model",json!("Apple")),("provider_verified",json!(true)),("duration_ms",json!(1)),("source_run_id",json!("fake-gateway"))] {
  let mut q=p.clone();q[key]=value;rejected(send(root.path(),key,"owner",&t,q));
 }
 assert_eq!(copies(root.path())?,0);assert_eq!(saved(root.path())?["revision"],0);Ok(())
}
#[test]
fn missing_corrupt_stale_generation_and_foreign_owner_uploads_never_publish()->anyhow::Result<()> {
 for mode in ["missing","corrupt","generation","owner","oversized"] {
  let (root,t,p)=fixture()?;let db=Connection::open(store::rxdb_store_path(root.path()))?;
  match mode {
   "missing"=>{db.execute("DELETE FROM ctox_business_os__desktop_file_chunks__v0 WHERE id LIKE '%000000'",[])?;},
   "corrupt"=>{db.execute("UPDATE ctox_business_os__desktop_file_chunks__v0 SET data=json_set(data,'$.data','corrupt') WHERE id LIKE '%000000'",[])?;},
   "generation"=>{db.execute("UPDATE ctox_business_os__desktop_files__v0 SET data=json_set(data,'$.content_generation_id','stale')",[])?;},
   "owner"=>{db.execute("UPDATE ctox_business_os__desktop_files__v0 SET data=json_set(data,'$.owner_id','foreign')",[])?;},
   _=>{db.execute("UPDATE ctox_business_os__desktop_files__v0 SET data=json_set(data,'$.size_bytes',8388609)",[])?;}
  }
  rejected(send(root.path(),mode,"owner",&t,p));assert_eq!(copies(root.path())?,0);assert_eq!(saved(root.path())?["state"],"preparing");
 }
 Ok(())
}
#[test]
fn malformed_wav_has_no_duration_attestation(){
 let valid=wav();assert_eq!(super::super::jour_fixe_local_narration::wav_duration(&valid).unwrap(),2000);
 for mode in ["truncated","size","format","rate","frames","data"] {
  let mut v=valid.clone();match mode {"truncated"=>{v.truncate(20);},"size"=>v[4]=0,"format"=>v[20]=7,"rate"=>v[24]=0,"frames"=>v[40]=255,_=>v[36]=b'x'};
  assert!(super::super::jour_fixe_local_narration::wav_duration(&v).is_err(),"{mode}");
 }
}
#[test]
fn stale_instance_project_deck_slide_text_revision_and_closed_meeting_are_rejected()->anyhow::Result<()> {
 for (field,value) in [("instance_id",json!("biz_other")),("project_id",json!("foreign-project")),("deck_revision",json!(2)),("slide_id",json!("other-slide")),("narration_text_sha256",json!("a".repeat(64))),("expected_revision",json!(99))] {
  let (root,t,mut p)=fixture()?;p[field]=value;rejected(send(root.path(),field,"owner",&t,p));assert_eq!(copies(root.path())?,0);
 }
 let (root,t,p)=fixture()?;change(root.path(),|v|v["state"]=json!("review"))?;
 rejected(send(root.path(),"closed","owner",&t,p));assert_eq!(copies(root.path())?,0);Ok(())
}
#[test]
fn receipt_failure_rolls_back_actual_file_chunks_and_meeting_before_retry()->anyhow::Result<()> {
 let (root,t,p)=fixture()?;open_store(root.path())?.execute_batch("CREATE TRIGGER fail_local_audio BEFORE INSERT ON business_command_domain_effects BEGIN SELECT RAISE(ABORT,'isolated receipt failure'); END;")?;
 rejected(send(root.path(),"failed","owner",&t,p.clone()));assert_eq!(copies(root.path())?,0);assert_eq!(saved(root.path())?["revision"],0);
 let chunks:i64=open_store(root.path())?.query_row("SELECT count(*) FROM business_records WHERE collection='desktop_file_chunks'",[],|r|r.get(0))?;assert_eq!(chunks,0);
 open_store(root.path())?.execute_batch("DROP TRIGGER fail_local_audio")?;
 let result=send(root.path(),"retry","owner",&t,p)?;assert_eq!(result["status"],"completed","{result}");assert_eq!(copies(root.path())?,1);Ok(())
}
#[test]
fn concurrent_commands_for_one_operation_copy_and_narrate_exactly_once()->anyhow::Result<()> {
 let (root,t,p)=fixture()?;let barrier=Arc::new(Barrier::new(2));let mut threads=vec![];
 for id in ["race-1","race-2"] {let path=root.path().to_owned();let token=t.clone();let payload=p.clone();let b=barrier.clone();threads.push(std::thread::spawn(move||{b.wait();send(&path,id,"owner",&token,payload)}));}
 let a=threads.remove(0).join().unwrap()?;let b=threads.remove(0).join().unwrap()?;
 assert_eq!(a["status"],"completed","{a}");assert_eq!(b["status"],"completed","{b}");assert_eq!(a["result"],b["result"]);
 assert_eq!(copies(root.path())?,1);assert_eq!(saved(root.path())?["revision"],1);Ok(())
}
#[test]
fn closed_or_rebound_scope_and_revoked_owner_cannot_replay_an_applied_receipt()->anyhow::Result<()> {
 for mode in ["closed","rebound","revoked"] {
  let (root,t,p)=fixture()?;let first=send(root.path(),"first","owner",&t,p.clone())?;assert_eq!(first["status"],"completed","{first}");
  match mode {"closed"=>change(root.path(),|v|v["state"]=json!("review"))?,"rebound"=>change(root.path(),|v|v["supervisor"]["workjet_thread_id"]=json!("missing"))?,_=>{open_store(root.path())?.execute("UPDATE business_users SET active=0 WHERE user_id='owner'",[])?;}}
  rejected(send(root.path(),"replay","owner",&t,p));assert!(command_plane::recover_applied_domain_effect_for_intake(root.path(),"first").is_err());assert_eq!(copies(root.path())?,1);
 }
 Ok(())
}
