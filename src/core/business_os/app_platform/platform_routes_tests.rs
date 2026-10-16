use super::*;
use ctox_store::{
    schema::{self, CollectionDefinition},
    types::{DbId, JobOptions, WriteJob},
};
use std::sync::Arc;

#[cfg(unix)]
#[tokio::test]
async fn e49_platform_rights_metadata_grants_and_restart_reply_boundary() {
    use sha2::{Digest, Sha256};
    println!(
        "E49_PLATFORM_SOURCE_SHA256 {:x}",
        Sha256::digest(include_bytes!("platform_routes.rs"))
    );
    let _scope = crate::service::store_startup::isolate_test_roots().await;
    let dir = tempfile::tempdir().unwrap();
    let mut stores = crate::service::store_runtime::DaemonStores::start(
        ctox_store::ipc::StoreEndpoint::UnixSocket {
            path: dir.path().join("ipc/store.sock"),
        },
        tokio::runtime::Handle::current(),
    )
    .unwrap();
    let db=stores.register("sync",DbId::new(dir.path().join("sync.sqlite")).unwrap(),true,Arc::new(|tx|{
        for name in ["business_commands","devices","machine_enrollments","business_users"] {
            schema::create_collection(tx,&CollectionDefinition{name:name.into(),app_id:"ctox-platform".into(),schema_ver:1,
                schema_json:json!({"version":1,"type":"object","primaryKey":"id","properties":{"id":{"type":"string"}},"required":["id"]}),
                class:"full".into(),client_limit:0,byte_budget:1048576,rights_ver:1,app_ver:"1".into()})?;
        }
        policy::sync_rights::initialize(tx).map_err(|e|StoreError::Invalid(e.to_string()))?;
        policy::sync_rights::replace(tx,1,&policy::base_release_rights(&[])).map_err(|e|StoreError::Invalid(e.to_string()))?;
        commands::put(tx,"business_users","owner",&json!({"id":"owner","active":true,"role":"admin"}))?;
        request_secret(tx,"request-a","workjet","owner","credentials","TEST_KEY")?;
        commands::put(tx,"devices","device-a",&json!({"id":"device-a","kind":"machine","user_id":"owner","machine_enrollment_id":"enrollment-a"}))?;
        commands::put(tx,"machine_enrollments","enrollment-a",&json!({"id":"enrollment-a","status":"green","created_by":"owner"}))?;
        Ok(())
    })).await.unwrap();
    let rights =
        policy::sync_rights::StoreRightsAuthority::attach(db.writer.clone(), "e49-isolated".into())
            .await
            .unwrap();
    let _policy = policy::bind_policy_authority(rights).unwrap();
    db.writer.submit(WriteJob::new(JobOptions::default(),|tx|{
        let admin=BusinessOsActor::new(Some("owner".into()),"admin");
        let user=BusinessOsActor::new(Some("owner".into()),"user");
        let kinds=["ctox.secret.request.list","ctox.secret.request.fulfill","ctox.secret.request.decline","ctox.workjet.remote_worker.admit","ctox.maintenance.restart"];
        for (i,kind) in kinds.iter().enumerate() {
            assert!(ctox_store::change_log::subscription::write_contract::routes::resolve(kind).is_ok());
            let c=CommandDocument{id:format!("denied-{i}"),app_id:"workjet".into(),module:"ctox-platform".into(),command_type:(*kind).into(),payload:json!({})};
            assert_eq!(execute(tx,&user,&c,100)?["code"],"forbidden","{kind}: removing the rights gate must fail this test");
        }
        let command=|id:&str,kind:&str,payload:Value|CommandDocument{id:id.into(),app_id:"workjet".into(),module:"ctox-platform".into(),command_type:kind.into(),payload};
        let listed=execute(tx,&admin,&command("list","ctox.secret.request.list",json!({})),100)?;
        assert_eq!(listed,json!({"requests":[{"request_id":"request-a","app_id":"workjet","user_id":"owner","status":"pending"}]}));
        assert_eq!(execute(tx,&admin,&command("fulfill","ctox.secret.request.fulfill",json!({"request_id":"request-a"})),100)?["reason"],"protected_secret_intake_required");
        assert_eq!(tx.query_row("SELECT state FROM _platform_secret_requests WHERE id='request-a'",[],|r|r.get::<_,String>(0))?,"pending");
        let declined=execute(tx,&admin,&command("decline","ctox.secret.request.decline",json!({"request_id":"request-a"})),100)?;
        assert_eq!(declined,json!({"request_id":"request-a","declined":true}));
        let grant=execute(tx,&admin,&command("grant","ctox.workjet.remote_worker.admit",json!({"device_id":"device-a","enrollment_id":"enrollment-a","scopes":["task.execute"],"ttl_ms":1000})),100)?;
        assert_eq!(grant["grant"]["actor_id"],"owner");assert_eq!(grant["grant"]["rights_ver"],policy::sync_rights::read(tx).map_err(|e|StoreError::Invalid(e.to_string()))?.rights_ver.to_string());
        assert!(worker_grant(tx,"grant",&admin,"workjet","device-a",101)?.is_some());
        assert!(worker_grant(tx,"grant",&admin,"foreign-app","device-a",101)?.is_none());
        assert!(worker_grant(tx,"grant",&admin,"workjet","device-a",1100)?.is_none());
        commands::put(tx,"devices","device-a",&json!({"id":"device-a","kind":"machine","user_id":"owner","machine_enrollment_id":"enrollment-a","revoked":true}))?;
        assert!(worker_grant(tx,"grant",&admin,"workjet","device-a",101)?.is_none());

        let denied=execute(tx,&admin,&command("foreign-grant","ctox.workjet.remote_worker.admit",json!({"device_id":"device-a","enrollment_id":"foreign","scopes":["task.execute"],"ttl_ms":1000})),100)?;
        assert_eq!(denied["reason"],"machine_binding_invalid");
        let restart=command("restart","ctox.maintenance.restart",json!({}));
        assert_eq!(execute(tx,&BusinessOsActor::new(Some("owner".into()),"chef"),&restart,100)?["code"],"forbidden");
        assert_eq!(execute(tx,&admin,&restart,100)?["awaiting_reply"],true);
        assert!(!claim_restart(tx,"restart")?,"restart may not precede the published reply");
        assert!(reply_published(tx,"restart")?);
        commands::put(tx,"business_users","owner",&json!({"id":"owner","active":true,"role":"user"}))?;
        assert!(!claim_restart(tx,"restart")?,"a role revoked after publication must fence restart");
        commands::put(tx,"business_users","owner",&json!({"id":"owner","active":true,"role":"admin"}))?;
        assert!(claim_restart(tx,"restart")?);assert!(!claim_restart(tx,"restart")?);
        // The models alias belongs to the native probe owner, not metadata.
        assert!(!handles("instance.providers.models.check"));
        assert!(ctox_store::change_log::subscription::write_contract::routes::resolve("instance.providers.models.check").is_ok());
        Ok(())
    })).unwrap().await.unwrap();
    stores.shutdown().await.unwrap();
}
