//! Additive peer-schema seam. Operates on the live database, never a second
//! connection/database and never a transport restart. The registry owner can
//! call `apply` directly after its schema commit (no directory poll required).
use rxdb::{
    plugins::replication_webrtc::query_fetch_handler::QueryFetchRegistry,
    rx_collection::RxCollection,
    rx_database::{RxCollectionCreator, RxDatabase},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tokio::sync::watch;

type Snapshot = Option<Arc<HashMap<String, RxCollectionCreator>>>;
static SCHEMA_COMMITS: OnceLock<Mutex<HashMap<PathBuf, watch::Sender<Snapshot>>>> = OnceLock::new();

fn commits(database: &Path) -> watch::Sender<Snapshot> {
    SCHEMA_COMMITS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .entry(database.to_path_buf())
        .or_insert_with(|| watch::channel(None).0)
        .clone()
}

/// Called by the daemon registry owner AFTER its schema transaction commits.
/// Use the canonical DbId path, shared with this peer's startup. A full snapshot
/// coalesces safely; it carries no credential or grant. Different stores never
/// receive each other's schema updates.
pub fn committed(database: &Path, creators: HashMap<String, RxCollectionCreator>) {
    commits(database).send_replace(Some(Arc::new(creators)));
}

pub fn subscribe(database: &Path) -> watch::Receiver<Snapshot> {
    let mut receiver = commits(database).subscribe();
    receiver.mark_changed();
    receiver
}

/// Existing generations are immutable here. Incompatible edits belong to the
/// schema migration runner; they must not replace a collection under readers.
pub async fn apply(
    database: &Arc<RxDatabase>,
    registries: &[Arc<QueryFetchRegistry>],
    mut creators: HashMap<String, RxCollectionCreator>,
) -> anyhow::Result<Vec<Arc<RxCollection>>> {
    // Retrying a partially completed registration must still publish every
    // successfully created handle to the registries. Do not skip known names.
    let existing: Vec<_> = {
        let collections = database.collections.lock();
        collections
            .iter()
            .filter(|(name, _)| creators.contains_key(*name))
            .map(|(_, collection)| Arc::clone(collection))
            .collect()
    };
    for collection in &existing {
        let requested = &creators
            .get(&collection.name)
            .expect("selected creator")
            .schema;
        let normalized = rxdb::rx_schema::create_rx_schema(
            requested.clone(),
            database.hash_function.clone(),
            true,
        )?;
        let current = collection
            .schema
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing active collection schema"))?;
        anyhow::ensure!(
            serde_json::to_value(&normalized.json_schema)?
                == serde_json::to_value(&current.json_schema)?,
            "schema generation migration required for {}",
            collection.name
        );
        creators.remove(&collection.name);
        for registry in registries {
            registry.register(Arc::clone(collection));
        }
    }
    let (added, failures) = database
        .add_collections_tolerant(creators)
        .await
        .map_err(|error| anyhow::anyhow!("online collection registration: {error}"))?;
    let mut registered = Vec::new();
    for (_, collection) in added {
        for registry in registries {
            registry.register(Arc::clone(&collection));
        }
        registered.push(collection);
    }
    if !failures.is_empty() {
        anyhow::bail!(
            "online collection registration incomplete: {}",
            failures
                .into_iter()
                .map(|(name, error)| format!("{name}: {error}"))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    Ok(registered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rxdb::{
        rx_database::{create_rx_database, RxDatabaseCreator},
        storage::sqlite::{get_rx_storage_sqlite, RxStorageSqliteSettings},
        types::{HashFunction, HashOutput, MangoQuery, RxJsonSchema},
    };
    use serde_json::json;
    use sha2::Digest;
    struct Hash;
    impl HashFunction for Hash {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("{:x}", sha2::Sha256::digest(input.as_bytes())) })
        }
    }
    #[tokio::test]
    async fn committed_collection_is_readable_without_reopening_database() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = create_rx_database(RxDatabaseCreator {
            name: "peer-hotplug".into(),
            storage: get_rx_storage_sqlite(RxStorageSqliteSettings {
                database_path: dir.path().join("sync.sqlite3"),
            }),
            multi_instance: false,
            password: None,
            hash_function: Arc::new(Hash),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: false,
            allow_slow_count: false,
        })
        .await?;
        let token = db.token.clone();
        let mut updates = subscribe(&dir.path().join("sync.sqlite3"));
        updates.borrow_and_update();
        let schema: RxJsonSchema = serde_json::from_value(json!({
            "version":0,"type":"object","primaryKey":"id",
            "properties":{"id":{"type":"string","maxLength":120},"title":{"type":"string"}},
            "required":["id","title"]
        }))?;
        let registry = Arc::new(QueryFetchRegistry::new(8));
        let started = std::time::Instant::now();
        committed(
            &dir.path().join("sync.sqlite3"),
            HashMap::from([(
                "hotplug_notes".into(),
                RxCollectionCreator {
                    schema: schema.clone(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]),
        );
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            updates.changed().await?;
            let snapshot = updates.borrow_and_update().clone().unwrap();
            apply(&db, std::slice::from_ref(&registry), (*snapshot).clone()).await?;
            let collection = registry
                .get("hotplug_notes")
                .ok_or_else(|| anyhow::anyhow!("new collection is not served"))?;
            assert!(
                Arc::ptr_eq(&collection.database, &db),
                "registration must retain the live database owner"
            );
            collection
                .insert(json!({"id":"one","title":"online"}))
                .await?;
            let document = collection
                .find_one(Some(MangoQuery {
                    selector: Some(json!({"id":"one"})),
                    ..Default::default()
                }))?
                .exec(true)
                .await?;
            assert_eq!(document.get("title"), Some(&json!("online")));
            anyhow::Ok(())
        })
        .await??;
        assert!(
            started.elapsed() <= std::time::Duration::from_secs(1),
            "online collection readability exceeded one second"
        );
        assert_eq!(
            db.token, token,
            "hot-plug must retain the same database incarnation"
        );
        eprintln!("hotplug-read-ms={}", started.elapsed().as_millis());
        let mut changed = schema;
        changed.version = 1;
        let error = apply(
            &db,
            &[registry],
            HashMap::from([(
                "hotplug_notes".into(),
                RxCollectionCreator {
                    schema: changed,
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]),
        )
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("active schema edit was silently skipped"))?;
        assert!(error
            .to_string()
            .contains("schema generation migration required"));
        db.close().await?;
        Ok(())
    }
}
