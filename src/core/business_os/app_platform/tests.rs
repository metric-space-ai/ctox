// Origin: CTOX
// License: AGPL-3.0-only

use super::super::super::policy::BusinessOsActor;
use super::{
    commands::{self, version_id},
    *,
};
use ctox_store::{
    change_log,
    schema::{self, CollectionDefinition, MigrationOutput, PublishLock},
    types::{DbId, JobOptions, WriteJob},
    writer::WriterHandle,
};
use ring::{
    rand::SystemRandom,
    signature::{self, Ed25519KeyPair, KeyPair},
};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};

use ctox_store::change_log::subscription::{
    protocol::{InstanceSigner, StreamKind},
    ReadAuthority, Subscription, SubscriptionKey,
};

struct StreamKey(Arc<Ed25519KeyPair>);
impl InstanceSigner for StreamKey {
    fn instance_id(&self) -> &str {
        "isolated-core"
    }
    fn key_id(&self) -> &str {
        "isolated-instance-key"
    }
    fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.0.sign(bytes).as_ref().to_vec())
    }
}

// The existing stream runs over the real snapshot reader and writer's retained
// commit wake. Its synthetic installer is authorized through native policy.
struct InstallerReadAuthority;
impl ReadAuthority for InstallerReadAuthority {
    fn authorize(
        &self,
        _: &rusqlite::Transaction<'_>,
        key: &SubscriptionKey,
    ) -> rusqlite::Result<()> {
        use super::super::super::policy::{self, BusinessOsPermission, BusinessOsScope};
        if key.participant != "owner"
            || key.collection != "business_apps"
            || !policy::evaluate(
                &admin(),
                BusinessOsPermission::AppsInstall,
                &BusinessOsScope::module("ctox-platform", false),
            )
            .allowed
        {
            return Err(rusqlite::Error::InvalidParameterName("forbidden".into()));
        }
        Ok(())
    }
    fn project(
        &self,
        _: &rusqlite::Transaction<'_>,
        _: &SubscriptionKey,
        doc: &change_log::ChangeDocument,
    ) -> rusqlite::Result<Option<Value>> {
        Ok(doc.body.clone())
    }
}
fn admin() -> BusinessOsActor {
    BusinessOsActor::new(Some("owner".into()), "admin")
}
fn manifest(version: u64) -> Value {
    json!({"app_id":"unknown-at-boot","title":"Runtime app","entry":"index.mjs","files":{"index.mjs":"a".repeat(64)},
        "collections":{"runtime_records":{"class":"full","client_limit":0,"byte_budget":4096,
            "schema":{"version":version,"type":"object","primaryKey":"id","properties":{"id":{"type":"string"},"label":{"type":"string"}},"required":["id"]}}}})
}
struct Fixture {
    stores: crate::service::store_runtime::DaemonStores,
    readers: ctox_store::read_pool::ReadPool,
    writer: WriterHandle,
    platform: Platform,
    public_key: Vec<u8>,
    stream_key: StreamKey,
    // Opened before any app installation: no reconnect/reset/rebuild permitted.
    reader: Connection,
    _directory: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("store.sqlite");
        let mut stores = crate::service::store_runtime::DaemonStores::start(
            ctox_store::ipc::StoreEndpoint::UnixSocket {
                path: directory.path().join("ipc/store.sock"),
            },
            tokio::runtime::Handle::current(),
        )
        .unwrap();
        let database = stores.register("sync", DbId::new(&path).unwrap(), true, Arc::new(|tx| {
            for name in ["business_commands","business_apps","business_app_versions","app_catalog","app_drafts","app_activations"] {
                schema::create_collection(tx, &CollectionDefinition {name:name.into(),app_id:"ctox-platform".into(),schema_ver:1,
                    schema_json:json!({"version":1,"type":"object","primaryKey":"id","properties":{"id":{"type":"string"}},"required":["id"]}),
                    class:"full".into(),client_limit:0,byte_budget:1_048_576,rights_ver:1,app_ver:"platform-1".into()})?;
            }
            Ok(())
        })).await.unwrap();
        let writer = database.writer.clone();
        let key = Arc::new(
            Ed25519KeyPair::from_pkcs8(
                Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                    .unwrap()
                    .as_ref(),
            )
            .unwrap(),
        );
        let public_key = key.public_key().as_ref().to_vec();
        let stream_key = StreamKey(key.clone());
        let platform = Platform::from_database(
            &database,
            Arc::new(move |bytes| {
                Ok(key
                    .sign(bytes)
                    .as_ref()
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect())
            }),
        );
        let platform = platform.unwrap();
        let reader = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        ctox_store::config::ConnectionConfig::default()
            .apply_reader(&reader)
            .unwrap();
        Self {
            stores,
            readers: database.readers,
            _directory: directory,
            writer,
            platform,
            public_key,
            stream_key,
            reader,
        }
    }
    async fn seed(&self, collection: &str, id: &str, body: Value) {
        let collection = collection.to_string();
        let id = id.to_string();
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                commands::put(tx, &collection, &id, &body)
            }))
            .unwrap()
            .await
            .unwrap();
    }
    async fn get(&self, collection: &str, id: &str) -> Option<Value> {
        let collection = collection.to_string();
        let id = id.to_string();
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                commands::get(tx, &collection, &id)
            }))
            .unwrap()
            .await
            .unwrap()
    }
    async fn release_input(&self, version: &str, content: Value, catalog: bool) {
        let bytes = serde_json::to_vec(&content).unwrap();
        let signature = self.platform.signer.as_ref()(&bytes).unwrap();
        self.seed("business_app_versions", &version_id("unknown-at-boot",version), json!({
            "id":version_id("unknown-at-boot",version),"app_id":"unknown-at-boot","app_ver":version,
            "manifest":content,"manifest_sha256":format!("{:x}",Sha256::digest(&bytes)),"signature":signature,"state":"published"
        })).await;
        if catalog {
            self.seed(
                "app_catalog",
                version,
                json!({"id":version,"app_id":"unknown-at-boot","app_ver":version,"source":"main"}),
            )
            .await;
        }
    }
    fn command(id: &str, kind: &str, base: Value, extra: Value) -> CommandDocument {
        let mut payload = json!({"app_id":"unknown-at-boot","base_app_ver":base});
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        CommandDocument {
            id: id.into(),
            app_id: "app-store".into(),
            module: "ctox-platform".into(),
            command_type: format!("ctox.app.{kind}"),
            payload,
        }
    }
    async fn run(&self, id: &str, kind: &str, base: Value, extra: Value) -> Value {
        self.platform
            .execute(admin(), Self::command(id, kind, base, extra), 1000)
            .unwrap()
            .await
            .unwrap()
    }
    async fn install(&self) -> String {
        self.release_input("release-1", manifest(1), true).await;
        let result = self
            .run(
                "install",
                "update",
                Value::Null,
                json!({"release_ver":"release-1"}),
            )
            .await;
        assert_eq!(result["status"], "accepted", "{result}");
        result["app_ver"].as_str().unwrap().into()
    }
    async fn draft(&self, base: Value, content: Value) {
        self.seed("app_drafts","draft",json!({"id":"draft","app_id":"unknown-at-boot","user_id":"owner","base_app_ver":base,
            "manifest_fields":content,"files":content["files"],"collections_schema":content["collections"]})).await;
    }
    fn read(&self, collection: &str, id: &str) -> Option<Value> {
        let table: String = self
            .reader
            .query_row(
                "SELECT table_name FROM _collections WHERE name=?1 AND state='active'",
                [collection],
                |r| r.get(0),
            )
            .unwrap();
        let value = self.reader.query_row(
            &format!("SELECT data FROM \"{table}\" WHERE id=?1"),
            [id],
            |r| r.get::<_, String>(0),
        );
        value.ok().map(|s| serde_json::from_str(&s).unwrap())
    }
}

#[tokio::test]
async fn publish_signed_draft_without_fabricating_client_activation() {
    let f = Fixture::new().await;
    let mut draft = manifest(1);
    draft["visible_roles"] = json!(["user"]);
    f.draft(Value::Null, draft).await;
    let admitted = Fixture::command(
        "publish",
        "publish",
        Value::Null,
        json!({"draft_id":"draft"}),
    );
    f.seed(
        "business_commands",
        "publish",
        json!({"id":"publish","app_id":admitted.app_id,"module":admitted.module,"command_type":admitted.command_type,
        "payload":admitted.payload,"client_context":{"actor":{"id":"owner"}},"status":"pending"}),
    )
    .await;
    let result = f
        .run(
            "publish",
            "publish",
            Value::Null,
            json!({"draft_id":"draft"}),
        )
        .await;
    assert_eq!(result["status"], "accepted");
    let app = f.read("business_apps", "unknown-at-boot").unwrap();
    assert_eq!(app["active_ver"], result["app_ver"]);
    assert_eq!(
        app["provenance"]["calling_app"], "app-store",
        "sender is document app_id, never module or payload.app_id"
    );
    assert_eq!(app["locally_modified"], true);
    assert_eq!(
        app["visible_roles"],
        json!(["chef", "admin"]),
        "manifest fields cannot assign native roles"
    );
    let version = f
        .get(
            "business_app_versions",
            &version_id("unknown-at-boot", result["app_ver"].as_str().unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(version["manifest"]["entry"], "index.mjs");
    let signature = version["signature"]
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|s| u8::from_str_radix(std::str::from_utf8(s).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    signature::UnparsedPublicKey::new(&signature::ED25519, &f.public_key)
        .verify(
            &serde_json::to_vec(&version["manifest"]).unwrap(),
            &signature,
        )
        .unwrap();
    let activations = f
        .writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            let table = schema::active_collection(tx, "app_activations")?
                .unwrap()
                .table;
            Ok(
                tx.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| {
                    r.get::<_, u64>(0)
                })?,
            )
        }))
        .unwrap()
        .await
        .unwrap();
    assert_eq!(activations, 0, "publishing is not client activation");
    assert_eq!(
        f.run(
            "publish",
            "publish",
            Value::Null,
            json!({"draft_id":"draft"})
        )
        .await,
        result,
        "replay must not republish"
    );
    let mut rebound = admitted;
    rebound.app_id = "another-app".into();
    assert!(
        f.platform
            .execute(admin(), rebound, 1000)
            .unwrap()
            .await
            .is_err(),
        "a command receipt cannot be rebound to another document app_id"
    );
}

#[tokio::test]
async fn update_hot_plugs_unknown_app_on_existing_connection() {
    let mut f = Fixture::new().await;
    assert!(f.read("business_apps", "unknown-at-boot").is_none());
    let readers = f.readers.clone();
    let mut stream = Subscription::new(
        readers.clone(),
        &f.writer,
        SubscriptionKey {
            participant: "owner".into(),
            collection: "business_apps".into(),
            scope_hash: "installer-apps".into(),
            gen: 1,
        },
        None,
    )
    .unwrap();
    let initial = stream
        .next_page(100, Arc::new(InstallerReadAuthority), &f.stream_key, || 1)
        .await
        .unwrap();
    assert_eq!(initial.kind, StreamKind::Page);
    assert!(initial.docs.is_empty());
    let mut feed = f.writer.subscribe_commits();
    feed.latest();
    let installed = f.install().await;
    let notification = tokio::time::timeout(Duration::from_secs(2), feed.changed())
        .await
        .unwrap()
        .unwrap();
    assert!(notification.head_seq > 0);
    let live = tokio::time::timeout(
        Duration::from_secs(2),
        stream.next_page(100, Arc::new(InstallerReadAuthority), &f.stream_key, || 2),
    )
    .await
    .expect("existing subscription must receive the unknown app")
    .unwrap();
    assert_eq!(live.kind, StreamKind::Live);
    assert!(live.seq_to > initial.seq_to);
    assert!(live.docs.iter().any(|doc| doc.id == "unknown-at-boot"
        && doc
            .body
            .as_ref()
            .is_some_and(|body| body["active_ver"] == installed)));
    assert_eq!(
        f.read("business_apps", "unknown-at-boot").unwrap()["active_ver"],
        installed
    );
    f.seed(
        "runtime_records",
        "persisted",
        json!({"id":"persisted","label":"survives"}),
    )
    .await;
    let mut changed = manifest(2);
    changed["title"] = json!("Updated while connected");
    changed["collections"]["runtime_records"]["rights"] = json!({"read":"admin","write":"admin"});
    f.release_input("release-2", changed, true).await;
    let result = f
        .run(
            "update",
            "update",
            json!(installed),
            json!({"release_ver":"release-2"}),
        )
        .await;
    assert_eq!(result["status"], "accepted");
    assert_eq!(
        f.read("business_apps", "unknown-at-boot").unwrap()["title"],
        "Updated while connected"
    );
    assert_eq!(
        f.read("runtime_records", "persisted").unwrap()["label"],
        "survives"
    );
    f.writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            let collection = schema::active_collection(tx, "runtime_records")?.unwrap();
            assert_eq!(
                collection.gen, 1,
                "compatible rights/schema changes preserve generation"
            );
            assert_eq!(
                collection.definition.rights_ver, 2,
                "rights changes invalidate old admissions"
            );
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let page = f
        .writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            let c = schema::active_collection(tx, "business_apps")?.unwrap();
            Ok(change_log::read_page(tx, "business_apps", c.gen, 0, 100)?)
        }))
        .unwrap()
        .await
        .unwrap();
    assert!(page.docs.iter().any(|d| d.id == "unknown-at-boot"
        && d.body.as_ref().unwrap()["title"] == "Updated while connected"));
    let updated = tokio::time::timeout(
        Duration::from_secs(2),
        stream.next_page(100, Arc::new(InstallerReadAuthority), &f.stream_key, || 3),
    )
    .await
    .expect("same subscription must adopt the update")
    .unwrap();
    assert_eq!(updated.kind, StreamKind::Live);
    assert!(updated.seq_to > live.seq_to);
    assert!(updated.docs.iter().any(|doc| doc.id == "unknown-at-boot"
        && doc
            .body
            .as_ref()
            .is_some_and(|body| body["title"] == "Updated while connected")));
    f.stores.shutdown().await.unwrap();
}

#[tokio::test]
async fn release_enters_catalog_and_stale_release_has_no_effect() {
    let f = Fixture::new().await;
    let installed = f.install().await;
    let result = f
        .run(
            "release",
            "release",
            json!(installed),
            json!({"channel":"stable"}),
        )
        .await;
    assert_eq!(result["status"], "accepted");
    assert!(f
        .read(
            "app_catalog",
            &format!("unknown-at-boot:{installed}:stable")
        )
        .is_some());
    assert_eq!(
        f.run(
            "stale-release",
            "release",
            json!("old"),
            json!({"channel":"bad"})
        )
        .await["code"],
        "stale_base"
    );
    assert!(f
        .read("app_catalog", &format!("unknown-at-boot:{installed}:bad"))
        .is_none());
}

#[tokio::test]
async fn rollback_publishes_new_version_and_retains_prior_versions() {
    let f = Fixture::new().await;
    let first = f.install().await;
    let mut m = manifest(2);
    m["title"] = json!("Changed");
    f.release_input("second", m, true).await;
    let second = f
        .run(
            "change",
            "update",
            json!(first),
            json!({"release_ver":"second"}),
        )
        .await["app_ver"]
        .clone();
    // Same schema semantics but an old schema version requires a real reverse
    // migration, so use an earlier title with the current compatible schema.
    let mut target = manifest(2);
    target["title"] = json!("Original title");
    f.release_input("rollback-target", target, false).await;
    let rolled = f
        .run(
            "rollback",
            "rollback",
            second.clone(),
            json!({"target_ver":"rollback-target"}),
        )
        .await;
    assert_eq!(rolled["status"], "accepted");
    assert_ne!(rolled["app_ver"], second);
    assert_ne!(rolled["app_ver"], "rollback-target");
    assert_eq!(
        f.read("business_apps", "unknown-at-boot").unwrap()["title"],
        "Original title"
    );
    assert!(f
        .get(
            "business_app_versions",
            &version_id("unknown-at-boot", &first)
        )
        .await
        .is_some());
}

#[tokio::test]
async fn remove_archives_without_deleting_even_after_retention() {
    let f = Fixture::new().await;
    let installed = f.install().await;
    f.seed(
        "runtime_records",
        "important",
        json!({"id":"important","label":"never delete"}),
    )
    .await;
    let result = f.run("remove", "remove", json!(installed), json!({})).await;
    assert_eq!(result["status"], "accepted");
    assert_eq!(
        f.read("business_apps", "unknown-at-boot").unwrap()["state"],
        "archived"
    );
    let retained = f
        .writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            let c = schema::collection(tx, "runtime_records", 1)?;
            assert_eq!(c.state, "archived");
            let value: String = tx.query_row(
                &format!("SELECT data FROM \"{}\" WHERE id='important'", c.table),
                [],
                |r| r.get(0),
            )?;
            assert!(
                schema::purge_generation_page(tx, "runtime_records", 1, 10_000_000, 100).is_err()
            );
            Ok(value)
        }))
        .unwrap()
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&retained).unwrap()["label"],
        "never delete"
    );
}

#[tokio::test]
async fn stale_base_rejects_every_command_including_null_install() {
    let f = Fixture::new().await;
    let installed = f.install().await;
    f.draft(Value::Null, manifest(1)).await;
    for (kind, extra) in [
        ("publish", json!({"draft_id":"draft"})),
        ("update", json!({"release_ver":"release-1"})),
        ("rollback", json!({"target_ver":"release-1"})),
        ("release", json!({"channel":"stale"})),
        ("remove", json!({})),
    ] {
        let result = f
            .run(&format!("stale-{kind}"), kind, Value::Null, extra)
            .await;
        assert_eq!(
            result,
            json!({"status":"rejected","code":"stale_base"}),
            "{kind}"
        );
    }
    assert_eq!(
        f.read("business_apps", "unknown-at-boot").unwrap()["active_ver"],
        installed
    );
}

#[tokio::test]
async fn not_found_prevents_installing_unreleased_version() {
    let f = Fixture::new().await;
    f.release_input("unreleased", manifest(1), false).await;
    let result = f
        .run(
            "missing",
            "update",
            Value::Null,
            json!({"release_ver":"unreleased"}),
        )
        .await;
    assert_eq!(result, json!({"status":"rejected","code":"not_found"}));
    assert!(f.read("business_apps", "unknown-at-boot").is_none());
}

#[tokio::test]
async fn locally_modified_requires_explicit_discard_and_archives_rollback_source() {
    let f = Fixture::new().await;
    f.draft(Value::Null, manifest(1)).await;
    let local = f
        .run("local", "publish", Value::Null, json!({"draft_id":"draft"}))
        .await["app_ver"]
        .clone();
    f.release_input("main", manifest(1), true).await;
    assert_eq!(
        f.run(
            "protect",
            "update",
            local.clone(),
            json!({"release_ver":"main"})
        )
        .await["code"],
        "locally_modified"
    );
    let replaced = f
        .run(
            "discard",
            "update",
            local.clone(),
            json!({"release_ver":"main","discard_local":true}),
        )
        .await;
    assert_eq!(replaced["status"], "accepted");
    let archived = f
        .get(
            "business_app_versions",
            &version_id("unknown-at-boot", local.as_str().unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(archived["origin"], "local");
    assert_eq!(archived["status"], "archived");
    assert_eq!(
        f.run(
            "restore-local",
            "rollback",
            replaced["app_ver"].clone(),
            json!({"target_ver":local})
        )
        .await["status"],
        "accepted"
    );
}

#[tokio::test]
async fn forbidden_uses_native_actor_and_draft_rights() {
    let f = Fixture::new().await;
    let installed = f.install().await;
    let user = BusinessOsActor::new(Some("other".into()), "user");
    let command = Fixture::command(
        "forbidden",
        "release",
        json!(installed),
        json!({"channel":"stable"}),
    );
    let result = f
        .platform
        .execute(user.clone(), command, 1000)
        .unwrap()
        .await
        .unwrap();
    assert_eq!(result, json!({"status":"rejected","code":"forbidden"}));
    let draft = json!({"user_id":"other","app_id":"unknown-at-boot"});
    assert!(can_read_draft(&user, &draft, None));
    assert!(can_write_draft(&user, &draft));
    assert!(can_read_draft(&admin(), &draft, None));
    assert!(!can_write_draft(&admin(), &draft));
    assert!(!can_read_draft(
        &BusinessOsActor::new(Some("stranger".into()), "user"),
        &draft,
        None
    ));
}

#[tokio::test]
async fn migration_required_keeps_active_schema_and_data_untouched() {
    let f = Fixture::new().await;
    f.release_input("new-schema", manifest(2), true).await;
    let current = f
        .run(
            "install",
            "update",
            Value::Null,
            json!({"release_ver":"new-schema"}),
        )
        .await["app_ver"]
        .clone();
    f.release_input("old-schema", manifest(1), true).await;
    let rejected = f
        .run(
            "older",
            "update",
            current.clone(),
            json!({"release_ver":"old-schema"}),
        )
        .await;
    assert_eq!(
        rejected,
        json!({"status":"rejected","code":"migration_required"})
    );
    assert_eq!(
        f.read("business_apps", "unknown-at-boot").unwrap()["active_ver"],
        current
    );
}

#[tokio::test]
async fn native_reverse_migration_waits_for_real_pages_before_switch() {
    let mut f = Fixture::new().await;
    f.release_input("new-schema", manifest(2), true).await;
    let current = f
        .run(
            "install",
            "update",
            Value::Null,
            json!({"release_ver":"new-schema"}),
        )
        .await["app_ver"]
        .clone();
    f.seed(
        "runtime_records",
        "kept",
        json!({"id":"kept","label":"retained"}),
    )
    .await;
    let mut old = manifest(1);
    old["collections"]["runtime_records"]["schema"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("label");
    old["collections"]["runtime_records"]["schema"]["properties"]["previous_label"] =
        json!({"type":"string"});
    old["migrations"] = json!({"runtime_records":{"from_ver":2,"steps":[{"op":"rename","from":"label","to":"previous_label"}]}});
    f.release_input("old-schema", old, false).await;
    // Native declarative rename migration. Not a payload permission flag.
    f.platform = f.platform.clone().with_migrations(Arc::new(|old,new,manifest| {
        old.schema_json.pointer("/properties/label/type").and_then(Value::as_str) == Some("string")
            && new.schema_json.pointer("/properties/previous_label/type").and_then(Value::as_str) == Some("string")
            && manifest["migrations"]["runtime_records"] == json!({"from_ver":2,"steps":[{"op":"rename","from":"label","to":"previous_label"}]})
    }));
    let pending = f
        .run(
            "reverse",
            "rollback",
            current.clone(),
            json!({"target_ver":"old-schema"}),
        )
        .await;
    assert_eq!(pending["status"], "preparing");
    assert_eq!(
        f.read("business_apps", "unknown-at-boot").unwrap()["active_ver"],
        current
    );
    assert!(
        f.platform
            .complete(admin(), "reverse".into(), 1001)
            .unwrap()
            .await
            .is_err(),
        "unmigrated generation must not activate"
    );
    let lock: PublishLock = serde_json::from_value(pending["publication"].clone()).unwrap();
    let lock_for_read = lock.clone();
    let page = f
        .writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            let migration = schema::migration(tx, &lock_for_read.operation_id, "runtime_records")?;
            Ok(change_log::read_page(
                tx,
                "runtime_records",
                migration.source_gen,
                migration.cursor,
                100,
            )?)
        }))
        .unwrap()
        .await
        .unwrap();
    // Transform outside the writer, then let P3 recheck source revisions.
    let outputs = page
        .docs
        .iter()
        .map(|d| {
            let mut body = d.body.clone().unwrap();
            body["previous_label"] = body.as_object_mut().unwrap().remove("label").unwrap();
            MigrationOutput::Upsert {
                rev: d.rev.clone(),
                body,
            }
        })
        .collect::<Vec<_>>();
    f.writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            Ok(schema::apply_migration_page(
                tx,
                &lock,
                "runtime_records",
                0,
                &page,
                &outputs,
                1,
            )?)
        }))
        .unwrap()
        .await
        .unwrap();
    assert_eq!(
        f.platform
            .complete(admin(), "reverse".into(), 1002)
            .unwrap()
            .await
            .unwrap()["status"],
        "accepted"
    );
    assert_eq!(
        f.read("runtime_records", "kept").unwrap()["previous_label"],
        "retained"
    );
    // Prepare a forward rename back, then revoke the caller before the pointer
    // switch. The native completion path must recheck the current actor too.
    let mut return_target = manifest(2);
    return_target["migrations"] = json!({"runtime_records":{"from_ver":3,"steps":[{"op":"rename","from":"previous_label","to":"label"}]}});
    f.release_input("return-schema", return_target, false).await;
    f.platform = f.platform.clone().with_migrations(Arc::new(|old,new,manifest| {
        old.schema_json.pointer("/properties/previous_label/type").and_then(Value::as_str) == Some("string")
            && new.schema_json.pointer("/properties/label/type").and_then(Value::as_str) == Some("string")
            && manifest["migrations"]["runtime_records"] == json!({"from_ver":3,"steps":[{"op":"rename","from":"previous_label","to":"label"}]})
    }));
    let active = f.read("business_apps", "unknown-at-boot").unwrap()["active_ver"].clone();
    let revoke = f
        .run(
            "revoke",
            "rollback",
            active.clone(),
            json!({"target_ver":"return-schema"}),
        )
        .await;
    assert_eq!(revoke["status"], "preparing");
    let lock: PublishLock = serde_json::from_value(revoke["publication"].clone()).unwrap();
    let lock_for_read = lock.clone();
    let page = f
        .writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            let m = schema::migration(tx, &lock_for_read.operation_id, "runtime_records")?;
            Ok(change_log::read_page(
                tx,
                "runtime_records",
                m.source_gen,
                m.cursor,
                100,
            )?)
        }))
        .unwrap()
        .await
        .unwrap();
    let outputs = page
        .docs
        .iter()
        .map(|d| {
            let mut body = d.body.clone().unwrap();
            body["label"] = body
                .as_object_mut()
                .unwrap()
                .remove("previous_label")
                .unwrap();
            MigrationOutput::Upsert {
                rev: d.rev.clone(),
                body,
            }
        })
        .collect::<Vec<_>>();
    f.writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            Ok(schema::apply_migration_page(
                tx,
                &lock,
                "runtime_records",
                0,
                &page,
                &outputs,
                1,
            )?)
        }))
        .unwrap()
        .await
        .unwrap();
    let revoked = BusinessOsActor::new(Some("owner".into()), "user");
    assert_eq!(
        f.platform
            .complete(revoked, "revoke".into(), 1003)
            .unwrap()
            .await
            .unwrap()["code"],
        "forbidden"
    );
    assert_eq!(
        f.read("business_apps", "unknown-at-boot").unwrap()["active_ver"],
        active
    );
    assert_eq!(
        f.read("runtime_records", "kept").unwrap()["previous_label"],
        "retained"
    );
}
