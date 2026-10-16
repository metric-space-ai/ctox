use super::*;
use ctox_store::schema::{self, CollectionDefinition};
use serde_json::json;
use sync_rights::{
    manifest_rules::ManifestRules,
    scope::{FieldMask, RowFilter},
    Operation, Right,
};
fn collection(tx: &Transaction<'_>, name: &str, owner: &str) {
    schema::create_collection(tx,&CollectionDefinition{name:name.into(),app_id:owner.into(),schema_ver:1,schema_json:json!({"version":0,"primaryKey":"id","type":"object","properties":{"id":{"type":"string","maxLength":160}},"required":["id"],"additionalProperties":true}),class:"replicated".into(),client_limit:100,byte_budget:1048576,rights_ver:1,app_ver:"v1".into()}).unwrap();
}
fn put(tx: &Transaction<'_>, name: &str, doc: Value) {
    let c = schema::active_collection(tx, name).unwrap().unwrap();
    let table = identifier(&c.table).unwrap();
    tx.execute(
        &format!("INSERT INTO {table}(id,data,rev,schema_ver,deleted) VALUES(?1,?2,'1',1,0)"),
        rusqlite::params![doc["id"].as_str().unwrap(), doc.to_string()],
    )
    .unwrap();
}
fn rules(export: bool, mask: FieldMask) -> sync_rights::RightsSnapshot {
    let exported = if export { json!(["read"]) } else { json!([]) };
    let desktop = ManifestRules::from_manifest(
        "desktop",
        "v1",
        ["desktop_files".into()].into(),
        &json!({"exports":{"desktop_files":exported}}),
    )
    .unwrap();
    let app = ManifestRules::from_manifest(
        "shell",
        "v1",
        ["notes".into()].into(),
        &json!({"foreign_collections":{"desktop_files":{"owner_app":"desktop","ops":["read"]}}}),
    )
    .unwrap();
    let platform =
        ManifestRules::from_manifest("platform", "v1", ["files".into()].into(), &json!({}))
            .unwrap();
    sync_rights::RightsSnapshot {
        rights_ver: 2,
        manifests: vec![desktop, app, platform],
        rules: ["desktop_files", "notes"]
            .into_iter()
            .map(|c| Right {
                role: "user".into(),
                collection: c.into(),
                row_filter: RowFilter::Eq {
                    path: "/owner_id".into(),
                    value: sync_rights::scope::FilterValue::ActorId,
                },
                field_mask: mask.clone(),
                ops: [Operation::Read].into(),
            })
            .collect(),
    }
}
#[test]
fn e30_native_files_require_exported_row_or_visible_actual_reference() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    schema::initialize(&tx).unwrap();
    for (n, o) in [
        ("files", "platform"),
        ("desktop_files", "desktop"),
        ("notes", "shell"),
    ] {
        collection(&tx, n, o);
    }
    let hash = "a".repeat(64);
    put(
        &tx,
        "files",
        json!({"id":"metadata","ref":hash,"size":99,"mime":"text/plain","owner_app":"other","owner_user":"bob"}),
    );
    let actor = BusinessOsActor::new(Some("alice".into()), "user");
    assert!(
        admit(&tx, &rules(true, FieldMask::All), &actor, "shell", &hash).is_err(),
        "a known hash is not permission"
    );
    put(
        &tx,
        "desktop_files",
        json!({"id":"desktop-entry","content_ref":hash,"owner_id":"bob"}),
    );
    assert!(
        admit(&tx, &rules(true, FieldMask::All), &actor, "shell", &hash).is_err(),
        "foreign row remains unreadable despite export"
    );
    let c = schema::active_collection(&tx, "desktop_files")
        .unwrap()
        .unwrap();
    let table = identifier(&c.table).unwrap();
    tx.execute(
        &format!("UPDATE {table} SET data=json_set(data,'$.owner_id','alice')"),
        [],
    )
    .unwrap();
    assert_eq!(
        admit(&tx, &rules(true, FieldMask::All), &actor, "shell", &hash)
            .unwrap()
            .size,
        99
    );
    assert!(
        admit(&tx, &rules(false, FieldMask::All), &actor, "shell", &hash).is_err(),
        "removing the owner's read export revokes bytes"
    );
    tx.execute(&format!("UPDATE {table} SET deleted=1"), [])
        .unwrap();
    put(
        &tx,
        "notes",
        json!({"id":"note","owner_id":"alice","attachment":{"ref":hash}}),
    );
    assert!(
        admit(&tx, &rules(false, FieldMask::All), &actor, "shell", &hash).is_ok(),
        "readable actual document holds a BlobRef"
    );
    assert!(
        admit(
            &tx,
            &rules(
                false,
                FieldMask::Fields {
                    paths: ["/id".into(), "/owner_id".into()].into()
                }
            ),
            &actor,
            "shell",
            &hash
        )
        .is_err(),
        "a hidden BlobRef grants no access"
    );
    let notes = schema::active_collection(&tx, "notes").unwrap().unwrap();
    tx.execute(
        &format!(
            "UPDATE {} SET data=json_set(data,'$.attachment.ref',?1)",
            identifier(&notes.table).unwrap()
        ),
        ["b".repeat(64)],
    )
    .unwrap();
    assert!(
        admit(&tx, &rules(false, FieldMask::All), &actor, "shell", &hash).is_err(),
        "caller cannot claim a reference absent from the actual row"
    );
    let files = schema::active_collection(&tx, "files").unwrap().unwrap();
    tx.execute(
        &format!(
            "UPDATE {} SET data=json_set(data,'$.owner_app','shell','$.owner_user','alice')",
            identifier(&files.table).unwrap()
        ),
        [],
    )
    .unwrap();
    for index in 0..65 {
        put(
            &tx,
            "desktop_files",
            json!({"id":format!("foreign-{index}"),"content_ref":hash,"owner_id":"bob"}),
        );
    }
    assert!(
        admit(&tx, &rules(false, FieldMask::All), &actor, "shell", &hash).is_ok(),
        "E30 own file needs no foreign collection export"
    );
    tx.execute(
        &format!(
            "UPDATE {} SET data=json_set(data,'$.owner_app','other','$.owner_user','other')",
            identifier(&files.table).unwrap()
        ),
        [],
    )
    .unwrap();
    put(
        &tx,
        "desktop_files",
        json!({"id":"zz-readable","content_ref":hash,"owner_id":"alice"}),
    );
    assert!(
        admit(&tx, &rules(true, FieldMask::All), &actor, "shell", &hash).is_ok(),
        "unreadable earlier candidates must not hide a later readable row"
    );
    let other = BusinessOsActor::new(Some("bob".into()), "user");
    assert!(
        admit(&tx, &rules(false, FieldMask::All), &other, "shell", &hash).is_err(),
        "app ownership alone cannot replace original user ownership"
    );
}

struct LiveSession(std::sync::atomic::AtomicBool);
impl WebRTCPublicationGuard for LiveSession {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        if !self.0.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(rxdb::rx_error::new_rx_error("revoked", None));
        }
        publish()
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e30_real_native_provider_rechecks_rights_before_any_frame() {
    use ctox_store::{testkit::StoreFixture, DbId};
    let root = tempfile::tempdir().unwrap();
    let fixture = StoreFixture::open(DbId::new(root.path().join("sync.sqlite3")).unwrap())
        .await
        .unwrap();
    let cas = Cas::open(root.path().join("cas")).unwrap();
    let input = root.path().join("bytes");
    std::fs::write(&input, b"real authorized bytes").unwrap();
    let blob = cas.import(input, "d".repeat(64)).await.unwrap();
    let reference = blob.reference().to_owned();
    let size = blob.size();
    fixture.write(Default::default(),move|tx|{
  schema::initialize(tx)?;for(n,o) in [("files","platform"),("desktop_files","desktop"),("notes","shell")]{collection(tx,n,o);}
  put(tx,"files",json!({"id":"file-metadata","ref":reference,"size":size,"owner_app":"other","owner_user":"bob","mime":"text/plain"}));
  put(tx,"desktop_files",json!({"id":"file-id","owner_id":"alice","content_ref":reference}));Ok(())
 }).await.unwrap();
    let authority =
        sync_rights::StoreRightsAuthority::attach(fixture.writer(), "native-e30".into())
            .await
            .unwrap();
    let r = rules(true, FieldMask::All);
    authority.replace(r.rules, r.manifests).await.unwrap();
    let policy = NativeStorePolicy::from_verified_peer(
        authority.clone(),
        VerifiedSyncPeer {
            actor: BusinessOsActor::new(Some("alice".into()), "user"),
            instance_id: "native-e30".into(),
            device_id: "device-alice".into(),
        },
        "shell".into(),
    )
    .unwrap();
    let session = Arc::new(LiveSession(std::sync::atomic::AtomicBool::new(true)));
    let access = NativeFileAccess::new(
        fixture.readers(),
        policy,
        "device-alice".into(),
        cas,
        session.clone(),
    );
    let request = TransferGet {
        kind: "transfer_get".into(),
        request_id: "request".into(),
        reference: blob.reference().into(),
        object_kind: "blob".into(),
        offset: 0,
        limit: 65536,
    };
    let prepared = access.resolve(&request).await.unwrap();
    let mut sent = false;
    prepared
        .publication
        .with_current(&mut || {
            sent = true;
            Ok(())
        })
        .unwrap();
    assert!(sent);
    let r = rules(false, FieldMask::All);
    authority.replace(r.rules, r.manifests).await.unwrap();
    let mut leaked = false;
    assert!(prepared
        .publication
        .with_current(&mut || {
            leaked = true;
            Ok(())
        })
        .is_err());
    assert!(
        !leaked,
        "rights revoked after CAS verification must fence the actual frame"
    );
    assert!(
        access.resolve(&request).await.is_err(),
        "a new range cannot reuse a revoked export"
    );
    let r = rules(true, FieldMask::All);
    authority.replace(r.rules, r.manifests).await.unwrap();
    let prepared = access.resolve(&request).await.unwrap();
    session.0.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(prepared
        .publication
        .with_current(&mut || {
            leaked = true;
            Ok(())
        })
        .is_err());
    assert!(
        !leaked,
        "retired native session must fence bytes independently of rights"
    );
    fixture.shutdown().await.unwrap();
}
