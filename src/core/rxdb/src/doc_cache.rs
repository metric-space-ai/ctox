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

// ref: rxdb/src/doc-cache.ts:61-178
/// Cache of document handles by primary key and revision state.
pub struct DocumentCache<T> {
    pub primary_path: String,
    document_creator: Arc<dyn Fn(RxDocumentData) -> Arc<T> + Send + Sync>,
    cache_item_by_doc_id: Mutex<HashMap<String, CacheItem<T>>>,
    subscription: Mutex<Option<JoinHandle<()>>>,
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
