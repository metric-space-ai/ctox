// Unit checks use the actual P3/P6 transaction helpers and an ephemeral test key.
//! Native codec contracts; these do not stand in for the composed daemon proof.
use super::*;
use ctox_app_runtime::core_host::{StoreCodec, StoreWrite};
use ctox_store::{
    change_log::subscription::{protocol::Seq, write_contract},
    schema::{self, CollectionDefinition},
};
use ring::{rand::SystemRandom, signature::Ed25519KeyPair};
struct TestKey(Ed25519KeyPair);
impl ctox_store::change_log::subscription::protocol::InstanceSigner for TestKey {
    fn instance_id(&self) -> &str {
        "isolated-runtime-test"
    }
    fn key_id(&self) -> &str {
        "ephemeral-test-key"
    }
    fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.0.sign(bytes).as_ref().to_vec())
    }
}
fn schema_json() -> Value {
    json!({"version":0,"primaryKey":"id","type":"object","properties":{"id":{"type":"string","maxLength":128},"n":{"type":"integer"}},"required":["id"]})
}
struct Fixture {
    db: Connection,
    codec: Arc<RuntimeCodec>,
    _root: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let mut db = Connection::open_in_memory().unwrap();
        let root = tempfile::tempdir().unwrap();
        let key = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let codec = Arc::new(RuntimeCodec {
            signer: Arc::new(TestKey(Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap())),
            cas: Cas::open(root.path()).unwrap(),
        });
        let tx = db.transaction().unwrap();
        schema::initialize(&tx).unwrap();
        for name in [
            "business_apps",
            "business_app_versions",
            "business_users",
            "business_commands",
            "rows",
            "files",
        ] {
            schema::create_collection(
                &tx,
                &CollectionDefinition {
                    name: name.into(),
                    app_id: if name == "rows" {
                        "app"
                    } else {
                        "ctox-platform"
                    }
                    .into(),
                    schema_ver: 1,
                    schema_json: schema_json(),
                    class: "replicated".into(),
                    client_limit: 100,
                    byte_budget: 1024 * 1024,
                    rights_ver: 1,
                    app_ver: "v1".into(),
                },
            )
            .unwrap();
        }
        let manifest = json!({"app_id":"app","app_ver":"v1","author":"owner","files":{},"handlers":{"run":"handlers/run.js"},"collections":{"rows":schema_json()},"command_rights":{"run":{"roles":["admin"]}}});
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let signature =
            super::super::commands::sign_manifest(codec.signer.as_ref(), &bytes).unwrap();
        for (name, id, doc) in [
            (
                "business_apps",
                "app",
                json!({"id":"app","state":"active","active_ver":"v1"}),
            ),
            (
                "business_app_versions",
                "app@v1",
                json!({"id":"app@v1","state":"published","manifest":manifest,"manifest_sha256":digest(&bytes),"signature":signature}),
            ),
            (
                "business_users",
                "owner",
                json!({"id":"owner","user_id":"owner","active":true,"role":"admin"}),
            ),
            (
                "business_commands",
                "cmd",
                json!({"id":"cmd","app_id":"app","actor_id":"owner","type":"run","payload":{},"status":"queued","created_at":chrono::Utc::now().to_rfc3339()}),
            ),
        ] {
            super::super::commands::put(&tx, name, id, &doc).unwrap();
        }
        sync_rights::initialize(&tx).unwrap();
        let policy = sync_rights::manifest_rules::ManifestRules::from_manifest(
            "app",
            "v1",
            ["rows".into()].into(),
            &manifest,
        )
        .unwrap();
        sync_rights::replace_with_manifests(
            &tx,
            1,
            &[sync_rights::Right {
                role: "admin".into(),
                collection: "rows".into(),
                row_filter: sync_rights::scope::RowFilter::All,
                field_mask: sync_rights::scope::FieldMask::All,
                ops: [
                    sync_rights::Operation::Read,
                    sync_rights::Operation::Write,
                    sync_rights::Operation::Delete,
                ]
                .into(),
            }],
            &[policy],
        )
        .unwrap();
        tx.commit().unwrap();
        Self {
            db,
            codec,
            _root: root,
        }
    }
    fn binding(&mut self) -> Value {
        let tx = self.db.transaction().unwrap();
        let gate = authority::ExecutorGate {
            codec: self.codec.clone(),
            participant: "native".into(),
        };
        let lease = write_contract::claim_command(&tx, "native", "cmd", None, now(), &gate)
            .unwrap()
            .unwrap();
        let manifest = self.codec.manifest(&tx, "app").unwrap();
        let mut command = document(&tx, "business_commands", "cmd").unwrap().unwrap();
        command["status"] = json!("running");
        store_command(&tx, &command).unwrap();
        command["app_ver"] = json!("v1");
        command["actor"] = json!({"id":"owner","role":"admin"});
        let b = json!({"command":command,"manifest":manifest,"fence":{"run_id":"app-runtime:cmd","participant":"native","fence":lease.fence.0,"expires_at_core_ms":lease.expires_at_core,"rights_ver":rights(&tx).unwrap().rights_ver}});
        tx.commit().unwrap();
        b
    }
}
#[test]
fn runtime_codec_rejects_unsigned_or_changed_active_release() {
    let mut f = Fixture::new();
    assert_eq!(
        f.codec.version(&f.db, "app", "v1").unwrap()["manifest"]["author"],
        "owner"
    );
    let tx = f.db.transaction().unwrap();
    let mut record = document(&tx, "business_app_versions", "app@v1")
        .unwrap()
        .unwrap();
    record["signature"] = json!("00".repeat(64));
    super::super::commands::put(&tx, "business_app_versions", "app@v1", &record).unwrap();
    tx.commit().unwrap();
    assert!(
        f.codec.version(&f.db, "app", "v1").is_err(),
        "tampered native release signature must deny executable code"
    );
}
#[test]
fn runtime_codec_binds_policy_and_fence_to_native_transaction() {
    let mut f = Fixture::new();
    let binding = f.binding();
    let tx = f.db.transaction().unwrap();
    let writes = [StoreWrite {
        collection: "rows".into(),
        op: json!({"type":"put","doc":{"id":"row","n":1}}),
    }];
    f.codec.write(&tx, &binding, None, &writes).unwrap();
    tx.commit().unwrap();
    assert_eq!(f.codec.read(&f.db, &binding, "rows").unwrap()[0]["n"], 1);
    let tx = f.db.transaction().unwrap();
    tx.execute(
        "UPDATE _command_leases SET fence=fence+1 WHERE command_id='cmd'",
        [],
    )
    .unwrap();
    assert!(
        f.codec.write(&tx, &binding, None, &writes).is_err(),
        "replaced native fence must prevent the writer effect"
    );
    tx.rollback().unwrap();
    let tx = f.db.transaction().unwrap();
    tx.execute("UPDATE _rights_state SET rights_ver=rights_ver+1", [])
        .unwrap();
    assert!(
        f.codec.write(&tx, &binding, None, &writes).is_err(),
        "policy revision must be checked inside the writer transaction"
    );
    tx.rollback().unwrap();
    let tx = f.db.transaction().unwrap();
    let mut denied = binding.clone();
    denied["command"]["actor"]["role"] = json!("chef");
    assert!(f.codec.write(&tx, &denied, None, &writes).is_err());
    tx.rollback().unwrap();
    let tx = f.db.transaction().unwrap();
    let result = f
        .codec
        .write(
            &tx,
            &binding,
            None,
            &[StoreWrite {
                collection: "rows".into(),
                op: json!({"type":"remove","id":"row"}),
            }],
        )
        .unwrap();
    assert_eq!(result[0]["status"], "accepted");
    tx.commit().unwrap();
    assert!(f.codec.read(&f.db, &binding, "rows").unwrap().is_empty());
    assert_eq!(
        f.db.query_row(
            "SELECT count(*) FROM _revs WHERE collection='rows' AND id='row' AND body IS NOT NULL",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    assert!(f
        .db
        .query_row(
            "SELECT data IS NULL AND deleted=1 FROM c_rows WHERE id='row'",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap());
    let _ = Seq(1);
}

#[test]
fn native_publish_preserves_v4_intent_and_activates_current_policy() {
    let mut f = Fixture::new();
    let tx = f.db.transaction().unwrap();
    for name in ["app_drafts", "app_catalog"] {
        schema::create_collection(
            &tx,
            &CollectionDefinition {
                name: name.into(),
                app_id: "ctox-platform".into(),
                schema_ver: 1,
                schema_json: schema_json(),
                class: "replicated".into(),
                client_limit: 100,
                byte_budget: 1024 * 1024,
                rights_ver: 1,
                app_ver: "v1".into(),
            },
        )
        .unwrap();
    }
    let mut r = sync_rights::read(&tx).unwrap();
    r.rules.extend(
        policy::base_release_rights(&[])
            .into_iter()
            .filter(|rule| rule.collection.starts_with("policy:")),
    );
    sync_rights::replace_with_manifests(&tx, r.rights_ver, &r.rules, &r.manifests).unwrap();
    let draft = json!({"id":"native-draft","app_id":"hot-app","author":"owner","manifest_fields":{"title":"Hot app"},"files":{},"collections_schema":{"hot_rows":{"schema":schema_json(),"class":"replicated","client_limit":100,"byte_budget":1048576}}});
    super::super::commands::put(&tx, "app_drafts", "native-draft", &draft).unwrap();
    let payload = json!({"app_id":"hot-app","base_app_ver":null,"draft_id":"native-draft"});
    let canonical = json!({"id":"native-publish","type":"ctox.app.publish","app_id":"ctox-platform","actor_id":"owner","created_at":chrono::Utc::now().to_rfc3339(),"status":"running","payload":payload,"module":"ctox-platform","command_type":"ctox.app.publish","client_context":{"actor":{"id":"owner"}}});
    store_command(&tx, &canonical).unwrap();
    let c = super::super::CommandDocument {
        id: "native-publish".into(),
        app_id: "ctox-platform".into(),
        module: "ctox-platform".into(),
        command_type: "ctox.app.publish".into(),
        payload,
    };
    let a = BusinessOsActor::new(Some("owner".into()), "admin");
    let outcome = super::super::commands::execute(
        &tx,
        &a,
        &c,
        now(),
        true,
        f.codec.signer.as_ref(),
        &|_, _, _| false,
    )
    .unwrap();
    assert_eq!(
        outcome["status"], "accepted",
        "native publication must use current transaction policy"
    );
    let saved = document(&tx, "business_commands", "native-publish")
        .unwrap()
        .unwrap();
    assert_eq!(
        saved["type"], canonical["type"],
        "native receipt must retain V4 command identity"
    );
    assert_eq!(saved["actor_id"], "owner");
    assert_eq!(saved["created_at"], canonical["created_at"]);
    assert_eq!(saved["status"], "done");
    let active = document(&tx, "business_apps", "hot-app").unwrap().unwrap();
    let policy = sync_rights::read(&tx).unwrap();
    assert!(
        policy
            .authorize_command("hot-app", "owner", "admin", "a.new.handler", false)
            .is_ok(),
        "newly published collections must have current native policy"
    );
    let projection = write_contract::read_command_projection(&tx, "native-publish").unwrap();
    // The source projection row exists even before the explicit global P9 cutover.
    assert_eq!(
        tx.query_row(
            "SELECT status FROM _command_projections WHERE command_id='native-publish'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "completed"
    );
    assert_eq!(
        f.codec
            .version(&tx, "hot-app", active["active_ver"].as_str().unwrap())
            .unwrap()["manifest"]["author"],
        "owner"
    );
    let _ = projection;
    tx.commit().unwrap();
}
