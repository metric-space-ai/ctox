//! Port of `src/doc-cache.ts`.
//!
//! T1 Rust re-design:
//! - JavaScript stores `WeakRef<RxDocument>` values and relies on
//!   `FinalizationRegistry` for eventual cleanup. Rust exposes the same
//!   ownership model directly via `Weak<T>` and explicit stale-entry pruning.
//! - The cache is generic over the cached document handle (`T`) so phase-6 can
//!   plug in `RxDocument` without reworking the cache. The creator returns
//!   `Arc<T>` because cached documents are shared values.
//! - Upstream defers change-stream processing via `requestIdlePromiseNoQueue`.
//!   This port updates the latest document data in a spawned async task. Callers
//!   can also use [`DocumentCache::apply_change_events`] in tests or custom
//!   schedulers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use serde_json::Value;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use crate::overwritable::OVERWRITABLE;
use crate::plugins::utils::utils_revision::get_height_of_revision;
use crate::rx_error::{new_rx_error, RxResult};
use crate::rxjs_compat::RxStream;
use crate::types::{RxDocumentData, RxStorageChangeEvent};

// ref: rxdb/src/doc-cache.ts:14-41
#[derive(Default)]
struct CacheItem<T> {
    by_rev: HashMap<String, Weak<T>>,
    latest: RxDocumentData,
}

/// After this many `get_cached_rx_documents` calls the cache opportunistically
/// sweeps the whole map for entries whose document handles have all been
/// dropped and removes them (freeing each dead entry's retained `latest`
/// clone). Upstream RxDB reclaims a `cacheItem` via `WeakRef` +
/// `FinalizationRegistry` when the document is GC'd; this port previously only
/// pruned dead revision handles *inside* an accessed entry and never removed
/// the outer map entry, so the map grew one dead entry per distinct doc-id ever
/// touched (churny ids: commands, queue tasks, rotating chunk generations),
/// pinning historical payloads in RAM. The on-access prune only reaches the
/// ids being accessed, so a periodic global sweep is what actually bounds
/// growth. Gating keeps the O(n) sweep amortized O(1) on the hot cache path.
const DOC_CACHE_SWEEP_INTERVAL: u64 = 256;

// ref: rxdb/src/doc-cache.ts:61-178
/// Cache of document handles by primary key and revision state.
pub struct DocumentCache<T> {
    pub primary_path: String,
    document_creator: Arc<dyn Fn(RxDocumentData) -> Arc<T> + Send + Sync>,
    cache_item_by_doc_id: Mutex<HashMap<String, CacheItem<T>>>,
    subscription: Mutex<Option<JoinHandle<()>>>,
    sweep_counter: AtomicU64,
}

/// Remove cache entries whose document handles have all been dropped, mirroring
/// upstream's GC of a whole `cacheItem`. For each surviving entry the dead
/// revision handles are pruned first; an entry is kept iff at least one live
/// handle remains (`by_rev` non-empty). An entry can only reach an empty
/// `by_rev` here by having every handle dropped: `get_cached_rx_documents`
/// always inserts a handle before releasing the lock, and `apply_change_events`
/// never inserts entries — so empty `by_rev` unambiguously means "no live
/// document references this id" and the retained `latest` is safe to free.
fn sweep_dead_cache_items<T>(items: &mut HashMap<String, CacheItem<T>>) {
    items.retain(|_, item| {
        item.by_rev.retain(|_, weak| weak.strong_count() > 0);
        !item.by_rev.is_empty()
    });
}

impl<T> DocumentCache<T>
where
    T: Send + Sync + 'static,
{
    /// Create a cache and subscribe to a stream of storage change-event batches.
    pub fn new(
        primary_path: impl Into<String>,
        changes_stream: RxStream<Vec<RxStorageChangeEvent>>,
        document_creator: Arc<dyn Fn(RxDocumentData) -> Arc<T> + Send + Sync>,
    ) -> Arc<Self> {
        let cache = Arc::new(Self {
            primary_path: primary_path.into(),
            document_creator,
            cache_item_by_doc_id: Mutex::new(HashMap::new()),
            subscription: Mutex::new(None),
            sweep_counter: AtomicU64::new(0),
        });

        let cache_for_task = Arc::clone(&cache);
        let handle = tokio::spawn(async move {
            let mut stream = changes_stream;
            while let Some(events) = stream.next().await {
                cache_for_task.apply_change_events(&events);
            }
        });
        *cache.subscription.lock() = Some(handle);

        cache
    }

    /// Create a cache without subscribing to a stream. Useful while phase-6
    /// wires collection-level streams, and for deterministic unit tests.
    pub fn new_without_stream(
        primary_path: impl Into<String>,
        document_creator: Arc<dyn Fn(RxDocumentData) -> Arc<T> + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new(Self {
            primary_path: primary_path.into(),
            document_creator,
            cache_item_by_doc_id: Mutex::new(HashMap::new()),
            subscription: Mutex::new(None),
            sweep_counter: AtomicU64::new(0),
        })
    }

    // ref: rxdb/src/doc-cache.ts:92-114
    /// Apply storage events to each known cache item's latest document data.
    pub fn apply_change_events(&self, events: &[RxStorageChangeEvent]) {
        if events.is_empty() {
            return;
        }
        let mut items = self.cache_item_by_doc_id.lock();
        for event in events {
            if let Some(item) = items.get_mut(&event.document_id) {
                if let Some(document_data) = event
                    .document_data
                    .clone()
                    .or_else(|| event.previous_document_data.clone())
                {
                    item.latest = document_data;
                }
            }
        }
    }

    // ref: rxdb/src/doc-cache.ts:145-156 + 190-248
    /// Return cached document handles for the given document data, creating
    /// missing handles and deduplicating equal revision/lwt states.
    pub fn get_cached_rx_documents(&self, docs_data: &[RxDocumentData]) -> RxResult<Vec<Arc<T>>> {
        let mut ret = Vec::with_capacity(docs_data.len());
        let mut items = self.cache_item_by_doc_id.lock();

        for doc_data in docs_data {
            let doc_id = document_id_from_primary(doc_data, &self.primary_path)?;
            let rev_key = revision_cache_key(doc_data)?;

            let item = items.entry(doc_id).or_insert_with(|| CacheItem {
                by_rev: HashMap::new(),
                latest: doc_data.clone(),
            });
            item.latest = doc_data.clone();

            if let Some(cached) = item.by_rev.get(&rev_key).and_then(Weak::upgrade) {
                ret.push(cached);
                continue;
            }

            item.by_rev.retain(|_, weak| weak.strong_count() > 0);
            let frozen = (OVERWRITABLE.load().deep_freeze_when_dev_mode)(doc_data.clone());
            let created = (self.document_creator)(frozen);
            item.by_rev.insert(rev_key, Arc::downgrade(&created));
            ret.push(created);
        }

        // Opportunistic, counter-gated global GC. `ret` still holds a strong
        // handle to every doc-id touched by this call, so their entries have a
        // live handle and are never swept here; only entries for ids whose
        // handles have all been dropped elsewhere are removed.
        if self.sweep_counter.fetch_add(1, Ordering::Relaxed) + 1 >= DOC_CACHE_SWEEP_INTERVAL {
            self.sweep_counter.store(0, Ordering::Relaxed);
            sweep_dead_cache_items(&mut items);
        }

        Ok(ret)
    }

    // ref: rxdb/src/doc-cache.ts:158-167
    pub fn get_cached_rx_document(&self, doc_data: &RxDocumentData) -> RxResult<Arc<T>> {
        self.get_cached_rx_documents(std::slice::from_ref(doc_data))?
            .into_iter()
            .next()
            .ok_or_else(|| new_rx_error("DOC_CACHE_EMPTY", Some(Value::Null)))
    }

    // ref: rxdb/src/doc-cache.ts:174-178
    pub fn get_latest_document_data(&self, doc_id: &str) -> RxResult<RxDocumentData> {
        self.cache_item_by_doc_id
            .lock()
            .get(doc_id)
            .map(|item| item.latest.clone())
            .ok_or_else(|| {
                new_rx_error(
                    "DOC_CACHE_MISSING",
                    Some(serde_json::json!({ "documentId": doc_id })),
                )
            })
    }

    // ref: rxdb/src/doc-cache.ts:180-185
    pub fn get_latest_document_data_if_exists(&self, doc_id: &str) -> Option<RxDocumentData> {
        self.cache_item_by_doc_id
            .lock()
            .get(doc_id)
            .map(|item| item.latest.clone())
    }

    pub fn close(&self) {
        if let Some(handle) = self.subscription.lock().take() {
            handle.abort();
        }
    }

    pub fn cached_document_count(&self) -> usize {
        self.cache_item_by_doc_id.lock().len()
    }

    /// Force a full GC sweep of dead cache entries. Test-only hook so tests can
    /// drive eviction deterministically without waiting for the call-count gate.
    #[cfg(test)]
    fn force_sweep(&self) {
        let mut items = self.cache_item_by_doc_id.lock();
        sweep_dead_cache_items(&mut items);
    }
}

// ref: rxdb/src/doc-cache.ts:255-260
pub fn map_documents_data_to_cache_docs<T>(
    doc_cache: &DocumentCache<T>,
    docs_data: &[RxDocumentData],
) -> RxResult<Vec<Arc<T>>>
where
    T: Send + Sync + 'static,
{
    doc_cache.get_cached_rx_documents(docs_data)
}

fn document_id_from_primary(doc_data: &Value, primary_path: &str) -> RxResult<String> {
    match doc_data.get(primary_path) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(v) => Ok(v.to_string()),
        None => Err(new_rx_error(
            "DOC_CACHE_PRIMARY",
            Some(serde_json::json!({ "primaryPath": primary_path, "document": doc_data })),
        )),
    }
}

fn revision_cache_key(doc_data: &Value) -> RxResult<String> {
    let rev = doc_data
        .get("_rev")
        .and_then(Value::as_str)
        .ok_or_else(|| new_rx_error("DOC_CACHE_REV", Some(doc_data.clone())))?;
    let revision_height = get_height_of_revision(rev)?;
    let lwt = doc_data
        .get("_meta")
        .and_then(|meta| meta.get("lwt"))
        .and_then(Value::as_f64)
        .ok_or_else(|| new_rx_error("DOC_CACHE_LWT", Some(doc_data.clone())))?;
    Ok(format!("{revision_height}:{lwt}"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Debug)]
    struct TestDoc {
        data: Value,
    }

    fn doc(id: &str, rev: &str, lwt: f64, value: i64) -> Value {
        serde_json::json!({
            "id": id,
            "value": value,
            "_rev": rev,
            "_deleted": false,
            "_attachments": {},
            "_meta": { "lwt": lwt }
        })
    }

    #[test]
    fn deduplicates_same_revision_state() {
        let created = Arc::new(AtomicUsize::new(0));
        let created_for_closure = Arc::clone(&created);
        let cache = DocumentCache::new_without_stream(
            "id",
            Arc::new(move |data| {
                created_for_closure.fetch_add(1, Ordering::SeqCst);
                Arc::new(TestDoc { data })
            }),
        );

        let input = doc("a", "1-token", 10.0, 1);
        let first = cache.get_cached_rx_document(&input).unwrap();
        let second = cache.get_cached_rx_document(&input).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.data.get("id").and_then(Value::as_str), Some("a"));
        assert_eq!(created.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn separates_same_revision_with_different_lwt() {
        let cache =
            DocumentCache::new_without_stream("id", Arc::new(|data| Arc::new(TestDoc { data })));

        let first = cache
            .get_cached_rx_document(&doc("a", "1-token", 10.0, 1))
            .unwrap();
        let second = cache
            .get_cached_rx_document(&doc("a", "1-token", 11.0, 1))
            .unwrap();

        assert!(!Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn evicts_cache_entry_after_all_handles_drop() {
        let cache =
            DocumentCache::new_without_stream("id", Arc::new(|data| Arc::new(TestDoc { data })));

        // Touch many distinct ids and drop every handle immediately. Without the
        // GC sweep these entries (and their `latest` clones) would linger.
        for i in 0..(DOC_CACHE_SWEEP_INTERVAL as i64 * 4) {
            let handle = cache
                .get_cached_rx_document(&doc(&format!("id-{i}"), "1-token", i as f64, i))
                .unwrap();
            drop(handle);
        }

        // The call-count-gated sweep must have bounded growth well below the
        // total number of distinct ids ever touched.
        assert!(
            cache.cached_document_count() < DOC_CACHE_SWEEP_INTERVAL as usize,
            "map should not grow unboundedly across distinct ids (got {})",
            cache.cached_document_count()
        );

        // A forced sweep with no live handles reclaims everything.
        cache.force_sweep();
        assert_eq!(cache.cached_document_count(), 0);
    }

    #[test]
    fn retains_entry_with_live_handle_and_still_resolves() {
        let cache =
            DocumentCache::new_without_stream("id", Arc::new(|data| Arc::new(TestDoc { data })));

        let live = cache
            .get_cached_rx_document(&doc("keep", "1-token", 1.0, 42))
            .unwrap();

        // Churn many other ids whose handles drop, then sweep.
        for i in 0..500i64 {
            let handle = cache
                .get_cached_rx_document(&doc(&format!("tmp-{i}"), "1-token", i as f64, i))
                .unwrap();
            drop(handle);
        }
        cache.force_sweep();

        // Only the live-handle entry survives.
        assert_eq!(cache.cached_document_count(), 1);

        // It still resolves to the same handle and its `latest` is intact.
        let again = cache
            .get_cached_rx_document(&doc("keep", "1-token", 1.0, 42))
            .unwrap();
        assert!(Arc::ptr_eq(&live, &again));
        assert_eq!(
            cache
                .get_latest_document_data("keep")
                .unwrap()
                .get("value")
                .and_then(Value::as_i64),
            Some(42)
        );
    }

    #[test]
    fn change_events_update_latest_known_data() {
        let cache =
            DocumentCache::new_without_stream("id", Arc::new(|data| Arc::new(TestDoc { data })));
        let original = doc("a", "1-token", 10.0, 1);
        let updated = doc("a", "2-token", 20.0, 2);

        let _ = cache.get_cached_rx_document(&original).unwrap();
        cache.apply_change_events(&[RxStorageChangeEvent {
            operation: "UPDATE".to_string(),
            document_id: "a".to_string(),
            document_data: Some(updated.clone()),
            previous_document_data: Some(original),
            is_local: false,
        }]);

        assert_eq!(
            cache
                .get_latest_document_data("a")
                .unwrap()
                .get("value")
                .and_then(Value::as_i64),
            Some(2)
        );
    }
}
