#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppBundle, RuntimePool};
    use rxdb::rx_database::{create_rx_database, RxCollectionCreator, RxDatabaseCreator};
    use rxdb::storage::sqlite::{get_rx_storage_sqlite, RxStorageSqliteSettings};
    use rxdb::types::{HashFunction, HashOutput};
    use sha2::{Digest, Sha256};
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use uuid::Uuid;

    struct Hash;
    impl HashFunction for Hash {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { rxdb::plugins::utils::utils_hash::native_sha256(&input) })
        }
    }
    struct Policy {
        allowed: AtomicBool,
    }
    impl RuntimeHost for Policy {
        fn authorize(&self, app: &str, actor: &Value, _op: &Value) -> Result<()> {
            ensure!(
                app == "test-store"
                    && matches!(actor["id"].as_str(), Some("a" | "b"))
                    && self.allowed.load(Ordering::SeqCst),
                "forbidden: actor policy"
            );
            Ok(())
        }
        fn exported(&self, _owner: &str, _collection: &str, _operation: &str) -> Result<bool> {
            Ok(false)
        }
        fn capability_available(&self, _name: &str) -> bool {
            false
        }
        fn invoke(&self, _app: &str, _actor: &Value, method: &str, _args: &Value) -> Result<Value> {
            ensure!(method == "log", "unavailable: native capability");
            Ok(Value::Null)
        }
    }
    impl LegacyStorePolicy for Policy {
        fn project_read(
            &self,
            _app: &str,
            actor: &Value,
            _collection: &str,
            document: &Value,
        ) -> Result<Option<Value>> {
            if document["owner"] != actor["id"] {
                return Ok(None);
            }
            let mut result = document.clone();
            result.as_object_mut().unwrap().remove("private");
            Ok(Some(result))
        }
        fn prepare_write(
            &self,
            _app: &str,
            actor: &Value,
            _collection: &str,
            before: Option<&Value>,
            proposed: Option<Value>,
        ) -> Result<Option<Value>> {
            ensure!(
                self.allowed.load(Ordering::SeqCst),
                "revoked: write permission"
            );
            if let Some(old) = before {
                ensure!(
                    old["owner"] == actor["id"],
                    "forbidden: another actor owns this record"
                );
            }
            Ok(proposed.map(|mut value| {
                value["owner"] = actor["id"].clone();
                value.as_object_mut().unwrap().remove("private");
                if let Some(secret) = before.and_then(|old| old.get("private")) {
                    value["private"] = secret.clone();
                }
                value
            }))
        }
    }
    struct Fixture {
        pool: RuntimePool,
        database: Arc<RxDatabase>,
        runtime: tokio::runtime::Runtime,
        policy: Arc<Policy>,
        _root: tempfile::TempDir,
    }
    impl Fixture {
        fn new() -> Self {
            let parent = std::env::var("CTOX_APP_RUNTIME_TEST_ROOT").unwrap();
            std::fs::create_dir_all(&parent).unwrap();
            let root = tempfile::Builder::new()
                .prefix("store-")
                .tempdir_in(parent)
                .unwrap();
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap();
            let database = runtime
                .block_on(create_rx_database(RxDatabaseCreator {
                    name: format!("handler-store-{}", Uuid::new_v4()),
                    storage: get_rx_storage_sqlite(RxStorageSqliteSettings {
                        database_path: root.path().join("store.sqlite"),
                    }),
                    multi_instance: false,
                    password: None,
                    hash_function: Arc::new(Hash),
                    options: HashMap::new(),
                    ignore_duplicate: false,
                    close_duplicates: false,
                    event_reduce: false,
                    allow_slow_count: false,
                }))
                .unwrap();
            runtime.block_on(database.add_collections(HashMap::from([("records".into(), RxCollectionCreator {
                schema: serde_json::from_value(json!({
                    "version":0,"primaryKey":"id","type":"object",
                    "properties":{"id":{"type":"string","maxLength":64},"title":{"type":"string"},"owner":{"type":"string"},"private":{"type":"string"}},
                    "required":["id","title","owner"]
                })).unwrap(), conflict_handler: None, options: HashMap::new(),
            })]))).unwrap();
            let policy = Arc::new(Policy {
                allowed: AtomicBool::new(true),
            });
            let adapter =
                LegacyStoreAdapter::new(database.clone(), runtime.handle().clone(), policy.clone())
                    .unwrap();
            let pool = RuntimePool::new(
                PathBuf::from(std::env::var("CTOX_APP_RUNTIME_TEST_NODE").unwrap()),
                root.path().join("ipc"),
                Arc::new(adapter),
            )
            .unwrap();
            let source = r#"
                export default async (input,ctx) => {
                    if(input.op) return await ctx.write('records',input.op);
                    return await ctx.read('records',input.query ?? {selector:{}});
                }
            "#;
            let hash = format!("{:x}", Sha256::digest(source.as_bytes()));
            pool.register(AppBundle::new(json!({
                "app_id":"test-store","collections":{"records":{}},
                "handlers":{"test.run":"handler.mjs"},"files":{"handler.mjs":hash},"capabilities_required":[]
            }), BTreeMap::from([("handler.mjs".into(),source.into())]),BTreeSet::new()).unwrap()).unwrap();
            Self {
                pool,
                database,
                runtime,
                policy,
                _root: root,
            }
        }
        fn call(&self, who: &str, input: Value) -> Result<Value> {
            self.pool
                .invoke("test-store", json!({"id":who}), "test.run", input)
        }
        fn seed(&self, rows: Vec<Value>) {
            let collection = self.database.collection("records").unwrap();
            self.runtime.block_on(collection.bulk_insert(rows)).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.pool.unregister("test-store").unwrap();
            self.runtime.block_on(self.database.close()).unwrap();
        }
    }

    #[test]
    fn node_reads_and_writes_through_real_sqlite_and_native_revision_queue() {
        let f = Fixture::new();
        let put = f
            .call(
                "a",
                json!({"op":{"type":"put","doc":{"id":"one","title":"first"},"base_rev":null}}),
            )
            .unwrap();
        assert_eq!(put["status"], "accepted");
        let rows = f.call("a", json!({})).unwrap();
        assert_eq!(rows[0]["title"], "first");
        let revision = rows[0]["rev"]
            .as_str()
            .expect("native source revision")
            .to_owned();
        assert_eq!(put["rev"], revision);
        assert_eq!(f.call("b", json!({})).unwrap(), json!([]));
        let patch = f.call("a",json!({"op":{"type":"patch","id":"one","base_rev":revision,"patch":{"title":"second"}}})).unwrap();
        assert_ne!(patch["rev"], put["rev"]);
        let stale = f.call("a",json!({"op":{"type":"patch","id":"one","base_rev":revision,"patch":{"title":"stale"}}})).unwrap_err();
        assert!(stale.to_string().starts_with("conflict:"));
        let collection = f.database.collection("records").unwrap();
        let native = f
            .runtime
            .block_on(
                collection
                    .storage_instance
                    .find_documents_by_id(&["one".into()], false),
            )
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(native["title"], "second");
        // A rejected stale modifier must not create a no-op native revision.
        assert_eq!(native["_rev"], patch["rev"]);
        let remove = f
            .call(
                "a",
                json!({"op":{"type":"remove","id":"one","base_rev":patch["rev"]}}),
            )
            .unwrap();
        assert_eq!(remove["status"], "accepted");
        assert_eq!(f.call("a", json!({})).unwrap(), json!([]));
    }
    #[test]
    fn core_row_and_field_policy_precedes_js_query_and_blocks_foreign_writes() {
        let f = Fixture::new();
        f.seed(vec![
            json!({"id":"own","title":"Visible","owner":"a","private":"masked"}),
            json!({"id":"other","title":"Other","owner":"b","private":"hidden"}),
        ]);
        let rows = f
            .call(
                "a",
                json!({"query":{"selector":{"title":{"$regex":"visible","$options":"i"}}}}),
            )
            .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert!(rows[0].get("private").is_none());
        assert_eq!(
            f.call(
                "a",
                json!({"query":{"selector":{"private":{"$exists":true}}}})
            )
            .unwrap(),
            json!([])
        );
        assert!(f
            .call(
                "a",
                json!({"op":{"type":"patch","id":"other","patch":{"title":"denied"}}})
            )
            .is_err());
        f.policy.allowed.store(false, Ordering::SeqCst);
        assert!(f
            .call(
                "a",
                json!({"op":{"type":"put","doc":{"id":"never","title":"denied"}}})
            )
            .is_err());
        let count = f
            .runtime
            .block_on(
                f.database
                    .collection("records")
                    .unwrap()
                    .count(None)
                    .unwrap()
                    .exec(false),
            )
            .unwrap();
        assert_eq!(count, json!(2));
    }
    #[test]
    fn legacy_store_has_no_silent_default_row_cap() {
        let f = Fixture::new();
        f.seed(
            (0..1200)
                .map(|n| json!({"id":format!("r{n:04}"),"title":"bulk","owner":"a"}))
                .collect(),
        );
        let rows = f
            .call(
                "a",
                json!({"query":{"selector":{"title":"bulk"},"sort":[{"id":"asc"}]}}),
            )
            .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1200);
        assert_eq!(rows[0]["id"], "r0000");
        assert_eq!(rows[1199]["id"], "r1199");
    }
}
