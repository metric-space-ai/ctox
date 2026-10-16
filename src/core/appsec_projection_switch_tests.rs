//! Sync v4 core-projection switch at the single AppSec projection entry point
//! (`appsec_state::project_cli_result`). The generic decision lives in
//! `ctox_store::app_catalog::core_projection_enabled`; these tests prove the
//! entry point honours it by counting what the projection chain writes.

#[cfg(test)]
mod tests {
    use crate::appsec_state::project_cli_result;
    use ctox_store::config::ConnectionConfig;
    use ctox_store::schema::{create_collection, CollectionDefinition};
    use rusqlite::Connection;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::{Path, PathBuf};

    const APP: &str = "appsec-pentest";

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

    /// A v4 catalog in `paths::sync_db`, writer held open like the daemon's.
    struct Catalog {
        writer: Connection,
        apps: String,
        versions: String,
    }

    impl Catalog {
        fn create(root: &Path) -> Self {
            let path = crate::paths::sync_db(root);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut writer = Connection::open(&path).unwrap();
            ConnectionConfig::default().apply_writer(&writer).unwrap();
            let tx = writer.transaction().unwrap();
            let apps = create_collection(&tx, &definition("business_apps"))
                .unwrap()
                .table;
            let versions = create_collection(&tx, &definition("business_app_versions"))
                .unwrap()
                .table;
            tx.commit().unwrap();
            Self {
                writer,
                apps,
                versions,
            }
        }
        fn put(&self, table: &str, id: &str, body: Value) {
            self.writer
                .execute(
                    &format!(
                        "INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,1,?3,0)
                         ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,data=excluded.data"
                    ),
                    rusqlite::params![id, uuid::Uuid::new_v4().to_string(), body.to_string()],
                )
                .unwrap();
        }
        fn publish(&self, ver: &str, manifest: Value) {
            let id = format!("{APP}@{ver}");
            self.put(
                &self.versions,
                &id,
                json!({"id":id,"app_id":APP,"app_ver":ver,"manifest":manifest}),
            );
        }
        fn activate(&self, ver: &str) {
            self.put(
                &self.apps,
                APP,
                json!({"id":APP,"title":"AppSec","active_ver":ver,"migrated_at_seq":"1"}),
            );
        }
    }

    fn exploit_fixture(root: &Path) -> PathBuf {
        let state = root.join("runtime/appsec/default");
        let exploits = state.join("exploits");
        fs::create_dir_all(&exploits).unwrap();
        let script = "#!/usr/bin/env python3\nprint('bounded proof')\n";
        fs::write(exploits.join("exploit_F-001.py"), script).unwrap();
        fs::write(exploits.join("index.md"), "# Exploits\n").unwrap();
        fs::write(
            exploits.join("index.json"),
            json!({
                "version": "ctox.appsec_pentest.exploit_index.v1",
                "exploits": [{
                    "finding_id": "F-001",
                    "script": exploits.join("exploit_F-001.py").display().to_string(),
                    "script_sha256": format!("{:x}", Sha256::digest(script.as_bytes())),
                }],
            })
            .to_string(),
        )
        .unwrap();
        state
    }

    fn project(root: &Path, state: &Path) -> Value {
        let args = vec![
            "ctox-appsec".to_string(),
            "--state-dir".to_string(),
            state.display().to_string(),
            "audit".to_string(),
            "run".to_string(),
        ];
        project_cli_result(root, &args, &json!({"ok": true, "command": "audit run"})).unwrap()
    }

    /// Rows the projection chain wrote into the core store (0 if never opened).
    fn projected_artifacts(root: &Path) -> i64 {
        let core = crate::paths::core_db(root);
        if !core.exists() {
            return 0;
        }
        Connection::open(core)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM appsec_artifacts", [], |r| r.get(0))
            .unwrap()
    }

    fn business_store_stat(root: &Path) -> Vec<Option<(std::time::SystemTime, u64)>> {
        let db = crate::paths::business_os_db(root);
        ["", "-wal"]
            .iter()
            .map(|suffix| {
                let path = PathBuf::from(format!("{}{suffix}", db.display()));
                fs::metadata(path)
                    .ok()
                    .map(|m| (m.modified().unwrap(), m.len()))
            })
            .collect()
    }

    fn assert_skipped(root: &Path, projection: &Value) {
        assert_eq!(projection["skipped"], json!("core_projection_disabled"));
        assert_eq!(projection["counts"], json!({}));
        assert!(projection.get("business_os_projection").is_none());
        assert_eq!(projected_artifacts(root), 0, "stage 1 wrote core rows");
        assert!(
            !crate::paths::core_db(root).exists(),
            "stage 1 opened the core store"
        );
        assert!(
            !crate::paths::business_os_db(root).exists(),
            "stage 2 opened the Business OS store"
        );
    }

    #[test]
    fn active_version_declaring_false_skips_the_whole_projection_chain() {
        let root = tempfile::tempdir().unwrap();
        let state = exploit_fixture(root.path());
        let catalog = Catalog::create(root.path());
        catalog.publish("2.0.0", json!({"app_id":APP,"core_projection":false}));
        catalog.activate("2.0.0");
        let projection = project(root.path(), &state);
        assert_skipped(root.path(), &projection);
    }

    #[test]
    fn missing_or_true_field_projects_as_before() {
        for manifest in [
            json!({"app_id":APP}),
            json!({"app_id":APP,"core_projection":true}),
        ] {
            let root = tempfile::tempdir().unwrap();
            let state = exploit_fixture(root.path());
            let catalog = Catalog::create(root.path());
            catalog.publish("1.0.0", manifest.clone());
            catalog.activate("1.0.0");
            let projection = project(root.path(), &state);
            assert!(projection.get("skipped").is_none(), "{manifest}");
            assert_eq!(
                projection.pointer("/counts/appsec_exploits"),
                Some(&json!(3))
            );
            assert_eq!(projected_artifacts(root.path()), 3, "{manifest}");
            assert!(projection.get("business_os_projection").is_some());
        }
    }

    #[test]
    fn switching_the_active_version_takes_effect_on_the_next_call() {
        let root = tempfile::tempdir().unwrap();
        let state = exploit_fixture(root.path());
        let catalog = Catalog::create(root.path());
        catalog.publish("1.0.0", json!({"app_id":APP}));
        catalog.publish("2.0.0", json!({"app_id":APP,"core_projection":false}));

        catalog.activate("2.0.0");
        assert_skipped(root.path(), &project(root.path(), &state));

        catalog.activate("1.0.0");
        let projection = project(root.path(), &state);
        assert!(projection.get("skipped").is_none());
        assert_eq!(projected_artifacts(root.path()), 3);

        // Back to the opt-out version: stage 1 must not recreate the core
        // store and stage 2 must not touch the Business OS store (stat only;
        // never read a live SQLite file through std::fs).
        fs::remove_file(crate::paths::core_db(root.path())).unwrap();
        let business = business_store_stat(root.path());
        catalog.activate("2.0.0");
        let projection = project(root.path(), &state);
        assert_eq!(projection["skipped"], json!("core_projection_disabled"));
        assert!(!crate::paths::core_db(root.path()).exists());
        assert_eq!(business_store_stat(root.path()), business);

        catalog.activate("1.0.0");
        assert_eq!(
            project(root.path(), &state).pointer("/counts/appsec_exploits"),
            Some(&json!(3))
        );
        assert_eq!(projected_artifacts(root.path()), 3);
    }

    #[test]
    fn unavailable_catalog_store_projects_as_before_and_creates_no_store() {
        let root = tempfile::tempdir().unwrap();
        let state = exploit_fixture(root.path());
        let projection = project(root.path(), &state);
        assert!(projection.get("skipped").is_none());
        assert_eq!(projected_artifacts(root.path()), 3);
        assert!(!crate::paths::sync_db(root.path()).exists());
    }
}
