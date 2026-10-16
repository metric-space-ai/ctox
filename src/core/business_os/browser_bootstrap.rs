//! Instance bootstrap material prepared by the foreground owner, never by HTTP.
//! Actor roles, grants, cookies and hello proofs deliberately stay outside this cache.
use super::store;
use crate::service::store_inputs::StoreInputs;
use std::{collections::HashMap, path::{Path, PathBuf}, sync::{Arc, Mutex, OnceLock, RwLock, Weak}};
use serde_json::Value;
use ctox_store::StoreError;

struct Committed {
    seq: u64,
    material: store::BrowserBootstrapMaterial,
    // Instance catalog only; never use this for an actor's authorization decision.
    _catalog: Value,
}
pub(crate) struct BrowserBootstrap {
    value: RwLock<Option<Arc<Committed>>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
static CACHES: OnceLock<Mutex<HashMap<PathBuf, Weak<BrowserBootstrap>>>> = OnceLock::new();
fn caches() -> &'static Mutex<HashMap<PathBuf, Weak<BrowserBootstrap>>> {
    CACHES.get_or_init(Mutex::default)
}
impl Drop for BrowserBootstrap {
    fn drop(&mut self) {
        if let Some(task) = self.task.get_mut().unwrap_or_else(|p| p.into_inner()).take() { task.abort(); }
    }
}
impl BrowserBootstrap {
    pub(crate) async fn start(inputs: &Arc<StoreInputs>) -> Result<Arc<Self>, StoreError> {
        // Attach before the first read: a commit racing preparation stays pending.
        let mut commits = inputs.sync.writer.subscribe_commits();
        let cache = Arc::new(Self { value: RwLock::new(None), task: Mutex::new(None) });
        cache.refresh(inputs).await?;
        caches().lock().unwrap_or_else(|p| p.into_inner())
            .insert(crate::paths::runtime_dir(&inputs.root), Arc::downgrade(&cache));
        let weak = Arc::downgrade(&cache);
        let owners = inputs.clone();
        let task = inputs.runtime.spawn(async move {
            while let Ok(notification) = commits.changed().await {
                let Some(cache) = weak.upgrade() else { break; };
                let current = cache.value.read().unwrap_or_else(|p| p.into_inner()).as_ref().map(|v| v.seq);
                if current.is_some_and(|seq| seq >= notification.head_seq) { continue; }
                // Keep serving the last committed value throughout preparation.
                if cache.refresh(&owners).await.is_err() {
                    // Never log credentials or arbitrary provider error strings.
                    eprintln!("[business-os] bootstrap preparation failed; retaining committed snapshot at seq {}", current.unwrap_or(0));
                }
            }
        });
        *cache.task.lock().unwrap_or_else(|p| p.into_inner()) = Some(task);
        Ok(cache)
    }
    async fn refresh(&self, inputs: &Arc<StoreInputs>) -> Result<(), StoreError> {
        let catalog = inputs.activation_snapshot(|tx| {
            let mut result = serde_json::Map::new();
            for collection in ["business_apps", "business_app_versions", "app_catalog"] {
                let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM _collections WHERE name=?1 AND state='active')", [collection], |r| r.get(0))?;
                let docs = if exists {
                    super::launch_context_v4::documents(tx, collection)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?
                } else { Vec::new() };
                result.insert(collection.into(), Value::Array(docs));
            }
            Ok(Value::Object(result))
        }).await?;
        let root = inputs.root.clone();
        let material = crate::persistence::run_core_sync(move || store::browser_bootstrap_material(&root))
            .await.map_err(|_| StoreError::Invalid("browser bootstrap preparation unavailable".into()))?;
        inputs.instance_signer()?;
        *self.value.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(Committed {seq: catalog.head_seq, material, _catalog: catalog.value}));
        Ok(())
    }
}
/// Memory-only. Missing readiness is visible and must never trigger inline work.
pub(crate) fn config(root: &Path, session: &str) -> anyhow::Result<store::BusinessOsSyncConfig> {
    let cache = caches().lock().unwrap_or_else(|p| p.into_inner())
        .get(&crate::paths::runtime_dir(root)).and_then(Weak::upgrade)
        .ok_or_else(|| anyhow::anyhow!("browser bootstrap owner unavailable"))?;
    let value = cache.value.read().unwrap_or_else(|p| p.into_inner()).clone()
        .ok_or_else(|| anyhow::anyhow!("browser bootstrap not ready"))?;
    Ok(value.material.for_session(session))
}
