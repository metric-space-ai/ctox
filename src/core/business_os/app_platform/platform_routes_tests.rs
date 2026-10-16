use super::*;
use ctox_store::{
    schema::{self, CollectionDefinition},
    types::{DbId, JobOptions, WriteJob},
};
use std::sync::Arc;

fn native_worker_binding_fixture(
    conn: &rusqlite::Connection,
) -> (
    Value,
    Value,
    Value,
    crate::business_os::machine_enrollment::ActiveDeviceEnrollment,
) {
    use crate::business_os::{devices, machine_enrollment as enrollment};
    conn.execute_batch("CREATE TABLE business_mobile_invites (
        invite_id_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL,
        redeemed_at_ms INTEGER, revoked_at_ms INTEGER, display_name TEXT NOT NULL,
        device_pairing_id TEXT, created_by_user_id TEXT, expires_at_ms INTEGER NOT NULL DEFAULT 0);
        INSERT INTO business_mobile_invites VALUES('fixture-hash','machine-user',10,10,NULL,'Worker','device-a','owner',0);").unwrap();
    let begun = enrollment::begin(
        &conn,
        "fixture-instance",
        "owner",
        "Worker",
        "local",
        Value::Null,
        Some("machine-user"),
        None,
        10,
    )
    .unwrap();
    let id = begun.document["id"].as_str().unwrap();
    let token = enrollment::JoinToken::decode(&begun.join_token).unwrap();
    let proof = enrollment::join_proof(&token.secret);
    let step =
        |key| enrollment::StepUpdate::from_payload(&json!({"key":key,"state":"done"})).unwrap();
    for key in ["transport_connected", "package_copied", "installed"] {
        enrollment::client_step(&conn, "owner", id, &step(key), 11).unwrap();
    }
    enrollment::machine_step(
        &conn,
        "fixture-instance",
        "machine-user",
        true,
        id,
        enrollment::MachineProof::Token(&proof),
        &step("service_started"),
        12,
    )
    .unwrap();
    for key in ["device_bound", "peer_connected"] {
        enrollment::machine_step(
            &conn,
            "fixture-instance",
            "machine-user",
            true,
            id,
            enrollment::MachineProof::BoundDevice,
            &step(key),
            13,
        )
        .unwrap();
    }
    let active = enrollment::heartbeat(&conn, "machine-user", true, id, 14).unwrap();
    assert_eq!(active["status"], "green");
    let device = devices::list(&conn).unwrap().remove(0);
    let binding = devices::active_machine_enrollment(conn, "device-a")
        .unwrap()
        .unwrap();
    assert_eq!(
        binding.enrollment_id, id,
        "T20: native resolver must retain the active enrollment identity"
    );
    let manifest: Value =
        serde_json::from_str(include_str!("../v4/ctox-platform.manifest.json")).unwrap();
    let device_schema = &manifest["collections"]["devices"]["schema"];
    assert_eq!(device_schema["additionalProperties"], false);
    assert!(
        device
            .as_object()
            .unwrap()
            .keys()
            .all(|key| device_schema["properties"].get(key).is_some()),
        "native device document must obey the actual closed base-release schema"
    );
    // A persisted authority revocation must remove the active association,
    // even while the device edge and an older projected device still exist.
    conn.execute(
        "UPDATE business_machine_enrollments SET status='cancelled' WHERE enrollment_id=?1",
        [id],
    )
    .unwrap();
    let closed = enrollment::heartbeat(&conn, "machine-user", true, id, 15).unwrap();
    assert!(
        enrollment::active_device_enrollments(&conn)
            .unwrap()
            .is_empty(),
        "T20: revoked enrollment has no native active binding"
    );
    assert!(devices::active_machine_enrollment(conn, "device-a")
        .unwrap()
        .is_none());
    conn.execute(
        "UPDATE business_machine_enrollments SET status='green' WHERE enrollment_id=?1",
        [id],
    )
    .unwrap();
    conn.execute(
        "UPDATE business_mobile_invites SET revoked_at_ms=16 WHERE device_pairing_id='device-a'",
        [],
    )
    .unwrap();
    assert!(
        enrollment::active_device_enrollments(&conn)
            .unwrap()
            .is_empty(),
        "T20: revoked device has no native active binding"
    );
    assert!(devices::list(&conn).unwrap().is_empty());
    conn.execute(
        "UPDATE business_mobile_invites SET revoked_at_ms=NULL WHERE device_pairing_id='device-a'",
        [],
    )
    .unwrap();
    (device, active, closed, binding)
}

#[cfg(unix)]
#[tokio::test]
async fn e49_platform_rights_metadata_grants_and_restart_reply_boundary() {
    use sha2::{Digest, Sha256};
    println!(
        "E49_PLATFORM_SOURCE_SHA256 {:x}",
        Sha256::digest(include_bytes!("platform_routes.rs"))
    );
    println!(
        "T20_DEVICES_SOURCE_SHA256 {:x}",
        Sha256::digest(include_bytes!("../devices.rs"))
    );
    println!(
        "T20_ENROLLMENT_SOURCE_SHA256 {:x}",
        Sha256::digest(include_bytes!("../machine_enrollment.rs"))
    );
    println!(
        "T20_PLATFORM_MOD_SOURCE_SHA256 {:x}",
        Sha256::digest(include_bytes!("mod.rs"))
    );
    println!(
        "T20_COMMANDS_SOURCE_SHA256 {:x}",
        Sha256::digest(include_bytes!("commands.rs"))
    );
    println!(
        "T20_TEST_SOURCE_SHA256 {:x}",
        Sha256::digest(include_bytes!("platform_routes_tests.rs"))
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
    let source = Arc::new(std::sync::Mutex::new(None));
    let prepared = source.clone();
    let bos = stores
        .register(
            "bos",
            DbId::new(dir.path().join("bos.sqlite")).unwrap(),
            false,
            Arc::new(move |tx| {
                *prepared.lock().unwrap() = Some(native_worker_binding_fixture(tx));
                Ok(())
            }),
        )
        .await
        .unwrap();
    let (native_device, native_enrollment, closed_enrollment, native_binding) =
        source.lock().unwrap().take().unwrap();
    let enrollment_id = native_enrollment["id"].as_str().unwrap().to_owned();
    let db=stores.register("sync",DbId::new(dir.path().join("sync.sqlite")).unwrap(),true,Arc::new(move |tx|{
        for name in ["business_commands","devices","machine_enrollments","business_users"] {
            let manifest:Value=serde_json::from_str(include_str!("../v4/ctox-platform.manifest.json")).unwrap();
            let mut schema_json=if matches!(name,"devices"|"machine_enrollments") {
                manifest["collections"][name]["schema"].clone()
            } else {json!({"type":"object","primaryKey":"id","properties":{"id":{"type":"string"}},"required":["id"]})};
            schema_json["version"]=json!(1);
            schema::create_collection(tx,&CollectionDefinition{name:name.into(),app_id:"ctox-platform".into(),schema_ver:1,
                schema_json,
                class:"full".into(),client_limit:0,byte_budget:1048576,rights_ver:1,app_ver:"1".into()})?;
        }
        policy::sync_rights::initialize(tx).map_err(|e|StoreError::Invalid(e.to_string()))?;
        policy::sync_rights::replace(tx,1,&policy::base_release_rights(&[])).map_err(|e|StoreError::Invalid(e.to_string()))?;
        commands::put(tx,"business_users","owner",&json!({"id":"owner","active":true,"role":"admin"}))?;
        request_secret(tx,"request-a","workjet","owner","credentials","TEST_KEY")?;
        commands::put(tx,"devices","device-a",&native_device)?;
        commands::put(tx,"machine_enrollments",native_enrollment["id"].as_str().unwrap(),&native_enrollment)?;
        Ok(())
    })).await.unwrap();
    let rights =
        policy::sync_rights::StoreRightsAuthority::attach(db.writer.clone(), "e49-isolated".into())
            .await
            .unwrap();
    let _policy = policy::bind_policy_authority(rights).unwrap();
    struct NativeTestKey(ring::signature::Ed25519KeyPair);
    impl ctox_store::change_log::subscription::protocol::InstanceSigner for NativeTestKey {
        fn instance_id(&self) -> &str {
            "isolated-worker-test"
        }
        fn key_id(&self) -> &str {
            "fixture-key"
        }
        fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
            Ok(self.0.sign(bytes).as_ref().to_vec())
        }
    }
    let signer = Arc::new(NativeTestKey(
        ring::signature::Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap(),
    ));
    let platform = Platform::new(db.writer.clone(), signer)
        .with_native_worker_database(&bos)
        .unwrap();
    let admin = BusinessOsActor::new(Some("owner".into()), "admin");
    let request = |id: &str| CommandDocument {
        id: id.into(),
        app_id: "workjet".into(),
        module: "ctox-platform".into(),
        command_type: "ctox.workjet.remote_worker.admit".into(),
        payload: json!({"device_id":"device-a","enrollment_id":enrollment_id,"scopes":["task.execute"],"ttl_ms":1000}),
    };
    let original = platform
        .dispatch(admin.clone(), request("native-grant"), 100)
        .await;
    assert_eq!(
        original["grant"]["device_id"], "device-a",
        "T20: productive adapter must consume the retained BOS resolver"
    );
    assert_eq!(
        platform
            .dispatch(admin.clone(), request("native-grant"), 100)
            .await,
        original,
        "native admission replay must retain command idempotency"
    );
    assert!(platform
        .worker_grant(
            "native-grant".into(),
            admin.clone(),
            "workjet".into(),
            "device-a".into(),
            101
        )
        .await
        .unwrap()
        .is_some());
    let revoke = enrollment_id.clone();
    bos.writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            tx.execute(
                "UPDATE business_machine_enrollments SET status='cancelled' WHERE enrollment_id=?1",
                [revoke],
            )?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    // Canonical docs deliberately remain green: a stale projection cannot
    // substitute for the current authoritative source at admission/execution.
    assert_eq!(
        platform
            .dispatch(admin.clone(), request("native-revoked"), 101)
            .await["reason"],
        "machine_binding_invalid",
        "T20: revoked native enrollment must not be admitted despite a stale green projection"
    );
    assert!(
        platform
            .worker_grant(
                "native-grant".into(),
                admin.clone(),
                "workjet".into(),
                "device-a".into(),
                101
            )
            .await
            .unwrap()
            .is_none(),
        "T20: revoked native enrollment must fence the execution consumer"
    );
    let restore = enrollment_id.clone();
    bos.writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            tx.execute(
                "UPDATE business_machine_enrollments SET status='green' WHERE enrollment_id=?1",
                [restore],
            )?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    bos.writer.submit(WriteJob::new(JobOptions::default(),|tx|{
        tx.execute("UPDATE business_mobile_invites SET revoked_at_ms=17 WHERE device_pairing_id='device-a'",[])?;Ok(())
    })).unwrap().await.unwrap();
    assert_eq!(
        platform
            .dispatch(admin.clone(), request("native-device-revoked"), 101)
            .await["reason"],
        "machine_binding_invalid",
        "T20: revoked native device must not be admitted despite a stale projection"
    );
    assert!(platform
        .worker_grant(
            "native-grant".into(),
            admin.clone(),
            "workjet".into(),
            "device-a".into(),
            101
        )
        .await
        .unwrap()
        .is_none());
    bos.writer.submit(WriteJob::new(JobOptions::default(),|tx|{
        tx.execute("UPDATE business_mobile_invites SET revoked_at_ms=NULL WHERE device_pairing_id='device-a'",[])?;Ok(())
    })).unwrap().await.unwrap();
    db.writer.submit(WriteJob::new(JobOptions::default(),move |tx|{
        let admin=BusinessOsActor::new(Some("owner".into()),"admin");
        let user=BusinessOsActor::new(Some("owner".into()),"user");
        let offered=vec!["files.ref".to_owned(),"ctox.maintenance.restart".to_owned()];
        assert_eq!(filter_session_capabilities(tx,"owner",offered.clone())?,offered);
        commands::put(tx,"business_users","owner",&json!({"id":"owner","active":true,"role":"chef"}))?;
        assert_eq!(filter_session_capabilities(tx,"owner",offered.clone())?,vec!["files.ref"],"E54: chef is not admin even when runtime.manage permits execution");
        assert_eq!(filter_session_capabilities(tx,"unknown",offered.clone())?,vec!["files.ref"],"E54: no authenticated current actor, no restart capability");
        commands::put(tx,"business_users","owner",&json!({"id":"owner","active":true,"role":"admin"}))?;
        let version=policy::sync_rights::read(tx)?.rights_ver;
        policy::sync_rights::replace(tx,version,&[])?;
        assert_eq!(filter_session_capabilities(tx,"owner",offered.clone())?,vec!["files.ref"],"E54: same-transaction rights revocation must fence even an Admin");
        let version=policy::sync_rights::read(tx)?.rights_ver;
        policy::sync_rights::replace(tx,version,&policy::base_release_rights(&[]))?;
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
        let grant=execute_with_binding(tx,&admin,&command("grant","ctox.workjet.remote_worker.admit",json!({"device_id":"device-a","enrollment_id":enrollment_id,"scopes":["task.execute"],"ttl_ms":1000})),100,&crate::business_os::machine_enrollment::DeviceEnrollmentResolution::Resolved(Some(native_binding.clone())))?;
        assert_eq!(grant["grant"]["actor_id"],"owner");assert_eq!(grant["grant"]["rights_ver"],policy::sync_rights::read(tx).map_err(|e|StoreError::Invalid(e.to_string()))?.rights_ver.to_string());
        assert!(worker_grant(tx,"grant",&admin,"workjet","device-a",101,Some(&native_binding))?.is_some());
        assert!(worker_grant(tx,"grant",&admin,"foreign-app","device-a",101,Some(&native_binding))?.is_none());
        assert!(worker_grant(tx,"grant",&admin,"workjet","device-a",1100,Some(&native_binding))?.is_none());
        commands::put(tx,"machine_enrollments",&enrollment_id,&closed_enrollment)?;
        assert!(worker_grant(tx,"grant",&admin,"workjet","device-a",101,Some(&native_binding))?.is_none(),"T20: revoked enrollment must fence an issued grant");
        let revoked=execute_with_binding(tx,&admin,&command("revoked-enrollment","ctox.workjet.remote_worker.admit",json!({"device_id":"device-a","enrollment_id":enrollment_id,"scopes":["task.execute"],"ttl_ms":1000})),101,&crate::business_os::machine_enrollment::DeviceEnrollmentResolution::Resolved(Some(native_binding.clone())))?;
        assert_eq!(revoked["reason"],"machine_binding_invalid","T20: revoked enrollment must not be admitted");


        let denied=execute_with_binding(tx,&admin,&command("foreign-grant","ctox.workjet.remote_worker.admit",json!({"device_id":"device-a","enrollment_id":"foreign","scopes":["task.execute"],"ttl_ms":1000})),100,&crate::business_os::machine_enrollment::DeviceEnrollmentResolution::Resolved(Some(native_binding.clone())))?;
        assert_eq!(denied["reason"],"machine_binding_invalid");
        let restart=command("restart","ctox.maintenance.restart",json!({}));
        assert_eq!(execute(tx,&BusinessOsActor::new(Some("owner".into()),"chef"),&restart,100)?["code"],"forbidden");
        let unavailable=execute(tx,&admin,&restart,100)?;
        assert_eq!(unavailable["code"],"unavailable","T20: restart without its publication/service consumer must be unavailable");
        assert_eq!(unavailable["reason"],"restart_publication_consumer_required");
        assert_eq!(tx.query_row("SELECT count(*) FROM _platform_maintenance_requests",[],|r|r.get::<_,i64>(0))?,0,"unavailable restart must create no effect intent");
        assert!(!reply_published(tx,"restart")?);
        assert!(!claim_restart(tx,"restart")?,"unavailable request cannot acquire a restart effect");
        // The models alias belongs to the native probe owner, not metadata.
        assert!(!handles("instance.providers.models.check"));
        assert!(ctox_store::change_log::subscription::write_contract::routes::resolve("instance.providers.models.check").is_ok());
        Ok(())
    })).unwrap().await.unwrap();
    stores.shutdown().await.unwrap();
}
