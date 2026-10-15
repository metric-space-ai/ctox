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
    sync::{Arc, OnceLock},
};
use tokio::sync::watch;

type Snapshot = Option<Arc<HashMap<String, RxCollectionCreator>>>;
static SCHEMA_COMMITS: OnceLock<watch::Sender<Snapshot>> = OnceLock::new();

fn commits() -> &'static watch::Sender<Snapshot> {
    SCHEMA_COMMITS.get_or_init(|| watch::channel(None).0)
}

/// Called by the daemon registry owner AFTER its schema transaction commits.
/// A full current snapshot coalesces safely; no credential or grant is carried.
pub fn committed(creators: HashMap<String, RxCollectionCreator>) {
    commits().send_replace(Some(Arc::new(creators)));
}

pub fn subscribe() -> watch::Receiver<Snapshot> {
    let mut receiver = commits().subscribe();
    // Also consume a registry commit that preceded this peer generation.
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
        let mut updates = subscribe();
        updates.borrow_and_update();
        let schema: RxJsonSchema = serde_json::from_value(json!({
            "version":0,"type":"object","primaryKey":"id",
            "properties":{"id":{"type":"string","maxLength":120},"title":{"type":"string"}},
            "required":["id","title"]
        }))?;
        let registry = Arc::new(QueryFetchRegistry::new(8));
        let started = std::time::Instant::now();
        committed(HashMap::from([(
            "hotplug_notes".into(),
            RxCollectionCreator {
                schema,
                conflict_handler: None,
                options: HashMap::new(),
            },
        )]));
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            updates.changed().await?;
            let snapshot = updates.borrow_and_update().clone().unwrap();
            apply(&db, &[registry.clone()], (*snapshot).clone()).await?;
            let collection = registry
                .get("hotplug_notes")
                .ok_or_else(|| anyhow::anyhow!("new collection is not served"))?;
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
        assert_eq!(
            db.token, token,
            "hot-plug must retain the same database incarnation"
        );
        eprintln!("hotplug-read-ms={}", started.elapsed().as_millis());
        db.close().await?;
        Ok(())
    }
}
