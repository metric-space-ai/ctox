//! Owner-prepared instance material. Cookies, principals and grants are never cached.
use super::store;
use crate::service::store_inputs::StoreInputs;
use ctox_store::StoreError;
use serde_json::Value;
use std::{collections::HashMap, path::{Path, PathBuf}, sync::{Arc, Mutex, OnceLock, RwLock, Weak}};
use tokio::sync::watch;
struct Committed {
    seq: u64,
    revision: u64,
    material: store::BrowserBootstrapMaterial,
    catalog: Arc<Value>,
}
pub(crate) struct BrowserBootstrap {
    value: RwLock<Option<Arc<Committed>>>,
    invalidated: watch::Sender<u64>,
    published: watch::Sender<(u64,u64)>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
static CACHES: OnceLock<Mutex<HashMap<PathBuf, Weak<BrowserBootstrap>>>> = OnceLock::new();
fn cache(root: &Path) -> Option<Arc<BrowserBootstrap>> {
    CACHES.get_or_init(Mutex::default).lock().unwrap_or_else(|p|p.into_inner())
        .get(&crate::paths::runtime_dir(root)).and_then(Weak::upgrade)
}
/// Called after a successful Runtime/Secrets commit; no credentials cross this seam.
pub(crate) fn invalidate(root: &Path) {
    if let Some(cache) = cache(root) {
        cache.invalidated.send_modify(|revision| *revision = revision.saturating_add(1));
    }
}
impl Drop for BrowserBootstrap {
    fn drop(&mut self) {
        if let Some(task) = self.task.get_mut().unwrap_or_else(|p|p.into_inner()).take() { task.abort(); }
    }
}
impl BrowserBootstrap {
    pub(crate) async fn start(inputs: &Arc<StoreInputs>) -> Result<Arc<Self>,StoreError> {
        let mut commits = inputs.sync.writer.subscribe_commits();
        let (invalidated, mut changes) = watch::channel(0);
        let (published, _) = watch::channel((0,0));
        let cache = Arc::new(Self {value:RwLock::new(None),invalidated,published,task:Mutex::new(None)});
        CACHES.get_or_init(Mutex::default).lock().unwrap_or_else(|p|p.into_inner())
            .insert(crate::paths::runtime_dir(&inputs.root), Arc::downgrade(&cache));
        cache.refresh(inputs).await?;
        let weak=Arc::downgrade(&cache); let owners=inputs.clone();
        let task=inputs.runtime.spawn(async move {
            loop {
                let expiry = weak.upgrade().and_then(|cache|
                    cache.value.read().unwrap_or_else(|p|p.into_inner()).as_ref()
                        .and_then(|value|value.material.refresh_after()));
                let expired = tokio::select! {
                    result=commits.changed() => { if result.is_err() {break;} false }
                    result=changes.changed() => { if result.is_err() {break;} false }
                    _=async { match expiry { Some(delay)=>tokio::time::sleep(delay).await, None=>std::future::pending::<()>().await } } => true,
                };
                let Some(cache)=weak.upgrade() else {break;};
                let head=commits.latest().map(|head|head.head_seq).unwrap_or(0);
                let revision=*changes.borrow_and_update();
                let current=cache.value.read().unwrap_or_else(|p|p.into_inner()).clone();
                if !expired && current.as_ref().is_some_and(|v|v.seq>=head && v.revision>=revision) {continue;}
                if cache.refresh(&owners).await.is_err() {
                    // Never log provider text or credential values.
                    eprintln!("[business-os] bootstrap refresh failed; last committed snapshot retained");
                }
            }
        });
        *cache.task.lock().unwrap_or_else(|p|p.into_inner())=Some(task);
        Ok(cache)
    }
    async fn refresh(&self, inputs:&Arc<StoreInputs>) -> Result<(),StoreError> {
        // Sample invalidation BEFORE preparing. A concurrent commit remains pending.
        let revision=*self.invalidated.borrow();
        let catalog=inputs.activation_snapshot(|tx| {
            let mut result=serde_json::Map::new();
            // Immutable app releases/catalog only. Assignments and grants stay live.
            for collection in ["business_app_versions","app_catalog"] {
                let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM _collections WHERE name=?1 AND state='active')",[collection],|r|r.get(0))?;
                let docs=if exists {super::launch_context_v4::documents(tx,collection).map_err(|_|rusqlite::Error::InvalidQuery)?} else {Vec::new()};
                result.insert(collection.into(),Value::Array(docs));
            }
            Ok(Value::Object(result))
        }).await?;
        let root=inputs.root.clone();
        let material=crate::persistence::run_core_sync(move||store::browser_bootstrap_material(&root))
            .await.map_err(|_|StoreError::Invalid("browser bootstrap preparation unavailable".into()))?;
        inputs.instance_signer()?;
        *self.value.write().unwrap_or_else(|p|p.into_inner())=Some(Arc::new(Committed {seq:catalog.head_seq,revision,material,catalog:Arc::new(catalog.value)}));
        self.published.send_replace((catalog.head_seq,revision));
        Ok(())
    }
}
pub(crate) struct Prepared {
    pub config:store::BusinessOsSyncConfig,
    pub catalog:Arc<Value>,
}
/// Memory-only, including fresh per-session HMAC. No inline initialization fallback.
pub(crate) fn prepared(root:&Path,session:&str)->anyhow::Result<Prepared> {
    crate::service::store_inputs::store_inputs(root)?.instance_signer()?;
    let cache=cache(root).ok_or_else(||anyhow::anyhow!("browser bootstrap owner unavailable"))?;
    let value=cache.value.read().unwrap_or_else(|p|p.into_inner()).clone()
        .ok_or_else(||anyhow::anyhow!("browser bootstrap not ready"))?;
    let started = std::time::Instant::now();
    let config = value.material.for_session(session);
    let config_us = started.elapsed().as_micros();
    let catalog_started = std::time::Instant::now();
    let catalog = value.catalog.clone();
    eprintln!("ctox_bootstrap_prepared_timing={}", serde_json::json!({"sync_config_us":config_us,"catalog_us":catalog_started.elapsed().as_micros(),"inline_computation":false}));
    Ok(Prepared {config,catalog})
}
#[cfg(test)]
pub(crate) async fn settle(inputs:&Arc<StoreInputs>)->anyhow::Result<()> {
    let expected=inputs.activation_snapshot(|_|Ok(())).await?.head_seq;
    let cache=cache(&inputs.root).ok_or_else(||anyhow::anyhow!("missing cache"))?;
    let revision=*cache.invalidated.borrow();
    let mut ready=cache.published.subscribe();
    tokio::time::timeout(std::time::Duration::from_secs(10),ready.wait_for(|(seq,rev)|*seq>=expected && *rev>=revision)).await??;
    Ok(())
}
