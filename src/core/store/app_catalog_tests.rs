use super::*;
use crate::config::ConnectionConfig;
use crate::schema::{create_collection, CollectionDefinition};
use serde_json::json;
use std::path::PathBuf;

const APP: &str = "appsec-pentest";

struct Store {
    _dir: tempfile::TempDir,
    path: PathBuf,
    /// Held open for the whole test like the daemon's store writer.
    writer: Connection,
    apps: String,
    versions: String,
}

fn definition(name: &str) -> CollectionDefinition {
    CollectionDefinition {
        name: name.into(),
        app_id: "ctox-platform".into(),
        schema_ver: 1,
        schema_json: json!({"type":"object"}),
        class: "replicated".into(),
        client_limit: 1000,
        byte_budget: 2_000_000,
        rights_ver: 1,
        app_ver: "1".into(),
    }
}

fn store() -> Store {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("business-os-rxdb.sqlite3");
    let mut writer = Connection::open(&path).unwrap();
    ConnectionConfig::default().apply_writer(&writer).unwrap();
    let tx = writer.transaction().unwrap();
    let apps = create_collection(&tx, &definition(APPS)).unwrap().table;
    let versions = create_collection(&tx, &definition(VERSIONS)).unwrap().table;
    tx.commit().unwrap();
    Store {
        _dir: dir,
        path,
        writer,
        apps,
        versions,
    }
}

impl Store {
    fn put(&self, table: &str, id: &str, body: Value) {
        self.writer
            .execute(
                &format!(
                    "INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,1,?3,0)
                     ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,data=excluded.data,deleted=0"
                ),
                rusqlite::params![id, uuid::Uuid::new_v4().to_string(), body.to_string()],
            )
            .unwrap();
    }
    fn delete(&self, table: &str, id: &str) {
        self.writer
            .execute(
                &format!("UPDATE {table} SET data=NULL,deleted=1,rev=?2 WHERE id=?1"),
                rusqlite::params![id, uuid::Uuid::new_v4().to_string()],
            )
            .unwrap();
    }
    /// Kernel layout: app document without app_id, version id `app@ver`.
    fn kernel_version(&self, app: &str, ver: &str, manifest: Value) {
        self.put(
            &self.versions,
            &format!("{app}@{ver}"),
            json!({"id":format!("{app}@{ver}"),"app_id":app,"app_ver":ver,"manifest":manifest}),
        );
    }
    fn kernel_activate(&self, app: &str, ver: &str) {
        self.put(
            &self.apps,
            app,
            json!({"id":app,"title":app,"active_ver":ver,"migrated_at_seq":"1"}),
        );
    }
    fn enabled(&self, app: &str) -> bool {
        core_projection_enabled(&self.path, app)
    }
}

#[test]
fn active_version_declaring_false_disables_core_projection() {
    let s = store();
    s.kernel_version(APP, "2.0.0", json!({"app_id":APP,"core_projection":false}));
    s.kernel_activate(APP, "2.0.0");
    assert!(!s.enabled(APP));
}

#[test]
fn missing_true_or_non_boolean_field_keeps_core_projection() {
    let s = store();
    for (ver, manifest) in [
        ("1.0.0", json!({"app_id":APP})),
        ("1.1.0", json!({"app_id":APP,"core_projection":true})),
        ("1.2.0", json!({"app_id":APP,"core_projection":"false"})),
        ("1.3.0", json!({"app_id":APP,"core_projection":0})),
        ("1.4.0", json!({"app_id":APP,"core_projection":null})),
    ] {
        s.kernel_version(APP, ver, manifest);
        s.kernel_activate(APP, ver);
        assert!(s.enabled(APP), "{ver}");
    }
}

#[test]
fn switching_the_active_version_takes_effect_immediately_without_restart() {
    let s = store();
    s.kernel_version(APP, "1.0.0", json!({"app_id":APP}));
    s.kernel_version(APP, "2.0.0", json!({"app_id":APP,"core_projection":false}));
    for _ in 0..3 {
        s.kernel_activate(APP, "2.0.0");
        assert!(!s.enabled(APP), "activated opt-out version");
        s.kernel_activate(APP, "1.0.0");
        assert!(s.enabled(APP), "rolled back to version without opt-out");
    }
}

#[test]
fn import_layout_with_colon_ids_and_app_id_fields_is_found_by_fields() {
    let s = store();
    let ver = "0.4.0";
    s.put(
        &s.versions,
        &format!("{APP}:{ver}"),
        json!({"id":format!("{APP}:{ver}"),"app_id":APP,"app_ver":ver,
               "manifest":{"app_id":APP,"core_projection":false},"state":"published"}),
    );
    // Import writes app_id on the app document; its id does not decide.
    s.put(
        &s.apps,
        "imported-row",
        json!({"id":"imported-row","app_id":APP,"active_ver":ver,"state":"active"}),
    );
    assert!(!s.enabled(APP));
}

#[test]
fn not_installed_inactive_or_deleted_apps_keep_core_projection() {
    let s = store();
    assert!(s.enabled(APP), "app not installed");
    s.kernel_version(APP, "2.0.0", json!({"core_projection":false}));
    assert!(s.enabled(APP), "version published but never activated");
    s.put(
        &s.apps,
        APP,
        json!({"id":APP,"app_id":APP,"active_ver":"2.0.0","state":"removed"}),
    );
    assert!(s.enabled(APP), "app document not active");
    s.kernel_activate(APP, "2.0.0");
    assert!(!s.enabled(APP));
    s.delete(&s.versions, &format!("{APP}@2.0.0"));
    assert!(s.enabled(APP), "active version document deleted");
    s.kernel_version(APP, "2.0.0", json!({"core_projection":false}));
    s.delete(&s.apps, APP);
    assert!(s.enabled(APP), "app document deleted");
}

#[test]
fn opt_out_is_scoped_to_its_own_app() {
    let s = store();
    s.kernel_version("other-app", "1.0.0", json!({"core_projection":false}));
    s.kernel_activate("other-app", "1.0.0");
    s.kernel_version(APP, "1.0.0", json!({}));
    s.kernel_activate(APP, "1.0.0");
    // Same version string, other app: must not leak through app_ver alone.
    assert!(!s.enabled("other-app"));
    assert!(s.enabled(APP));
}

#[test]
fn ambiguous_catalog_keeps_core_projection() {
    let s = store();
    s.kernel_version(APP, "1.0.0", json!({}));
    s.kernel_version(APP, "2.0.0", json!({"core_projection":false}));
    s.kernel_activate(APP, "2.0.0");
    s.put(
        &s.apps,
        "duplicate",
        json!({"id":"duplicate","app_id":APP,"active_ver":"1.0.0"}),
    );
    assert!(s.enabled(APP), "two app documents disagree on active_ver");
    s.delete(&s.apps, "duplicate");
    assert!(!s.enabled(APP));
    // Two version documents for the active version must agree on the opt-out.
    s.put(
        &s.versions,
        &format!("{APP}:2.0.0"),
        json!({"app_id":APP,"app_ver":"2.0.0","manifest":{}}),
    );
    assert!(s.enabled(APP), "kernel and import manifests disagree");
}

#[test]
fn unavailable_store_keeps_core_projection_and_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("runtime/business-os-rxdb.sqlite3");
    assert!(core_projection_enabled(&missing, APP));
    assert!(!missing.exists(), "the check must never create a store");
    assert!(!missing.parent().unwrap().exists());

    let garbage = dir.path().join("garbage.sqlite3");
    std::fs::write(&garbage, b"this is not a sqlite database at all.....").unwrap();
    assert!(core_projection_enabled(&garbage, APP));

    // Legacy RxDB file at the same path: no v4 catalog registry.
    let legacy = dir.path().join("legacy.sqlite3");
    Connection::open(&legacy)
        .unwrap()
        .execute_batch("CREATE TABLE business_apps(id TEXT PRIMARY KEY, data TEXT)")
        .unwrap();
    assert!(core_projection_enabled(&legacy, APP));
}

#[test]
fn catalog_with_only_apps_collection_keeps_core_projection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.sqlite3");
    let mut writer = Connection::open(&path).unwrap();
    ConnectionConfig::default().apply_writer(&writer).unwrap();
    let tx = writer.transaction().unwrap();
    let apps = create_collection(&tx, &definition(APPS)).unwrap().table;
    tx.execute(
        &format!("INSERT INTO {apps}(id,rev,schema_ver,data,deleted) VALUES(?1,'r',1,?2,0)"),
        rusqlite::params![APP, json!({"id":APP,"active_ver":"2.0.0"}).to_string()],
    )
    .unwrap();
    tx.commit().unwrap();
    assert!(core_projection_enabled(&path, APP));
}
