//! Port of `src/rx-query-single-result.ts`.
//!
//! Upstream `RxQuerySingleResult<RxDocType>` lazily wraps storage-side
//! `docsData` into `RxDocument` instances. The Rust port keeps JSON accessors
//! for existing CTOX call sites, while query execution can now also expose the
//! cached `RxDocument` handles.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::doc_cache::map_documents_data_to_cache_docs;
use crate::plugins::utils::utils_time::now;
use crate::rx_collection::RxCollection;
use crate::rx_document::RxDocument;
use crate::rx_error::{new_rx_error, RxResult};

// ref: rxdb/src/rx-query-single-result.ts:17-101
/// Result of one execution of a query against the storage.
pub struct RxQuerySingleResult {
    /// Primary-key path of the underlying schema — used by [`docs_data_map`].
    primary_path: String,
    /// Storage-returned docs (each `RxDocumentData` is a `Value`).
    docs_data: Vec<Value>,
    /// Cached document handles materialized like upstream `documents`.
    documents: Option<Vec<Arc<RxDocument>>>,
    /// `count` queries set this explicitly; otherwise = `docs_data.len()`.
    pub count: u64,
    /// `now()` at construction time. Used by `RxQuery._ensureEqual` to skip
    /// re-emitting the same result on repeat subscription.
    pub time: f64,
}

impl RxQuerySingleResult {
    pub fn new(primary_path: impl Into<String>, docs_data: Vec<Value>, count: u64) -> Self {
        Self {
            primary_path: primary_path.into(),
            docs_data,
            documents: None,
            count,
            time: now(),
        }
    }

    pub fn from_collection(
        collection: &Arc<RxCollection>,
        docs_data: Vec<Value>,
        count: u64,
    ) -> RxResult<Self> {
        let primary_path = collection.primary_path().ok_or_else(|| {
            new_rx_error(
                "QU_SCHEMA",
                Some(serde_json::json!({ "collection": collection.name })),
            )
        })?;
        let documents = map_documents_data_to_cache_docs(collection.doc_cache()?, &docs_data)?;
        Ok(Self {
            primary_path,
            docs_data,
            documents: Some(documents),
            count,
            time: now(),
        })
    }

    // ref: rxdb/src/rx-query-single-result.ts:42-48 docsData
    pub fn docs_data(&self) -> &[Value] {
        &self.docs_data
    }

    pub fn documents(&self) -> RxResult<&[Arc<RxDocument>]> {
        self.documents.as_deref().ok_or_else(|| {
            new_rx_error(
                "QU_DOCUMENTS",
                Some(serde_json::json!({
                    "message": "query result was not materialized with a collection",
                })),
            )
        })
    }

    // ref: rxdb/src/rx-query-single-result.ts:51-62 docsDataMap
    /// `primary → docData` map; equivalent to upstream `docsDataMap` but
    /// built eagerly (cheap on the hot path: O(n) on `docs_data`).
    pub fn docs_data_map(&self) -> HashMap<String, Value> {
        let mut map = HashMap::with_capacity(self.docs_data.len());
        for d in self.docs_data.iter() {
            if let Some(id) = d.get(&self.primary_path).and_then(|v| v.as_str()) {
                map.insert(id.to_string(), d.clone());
            }
        }
        map
    }

    // ref: rxdb/src/rx-query-single-result.ts:64-76 docsMap
    pub fn docs_map(&self) -> RxResult<HashMap<String, Arc<RxDocument>>> {
        let documents = self.documents()?;
        let mut map = HashMap::with_capacity(documents.len());
        for doc in documents {
            map.insert(doc.primary()?, Arc::clone(doc));
        }
        Ok(map)
    }

    /// `docs_data` length — handy for the common `find()` case where
    /// `count == docs_data.len()`.
    pub fn doc_count(&self) -> usize {
        self.docs_data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn time_is_set_at_construction() {
        let before = now();
        let r = RxQuerySingleResult::new("id", vec![], 0);
        let after = now();
        assert!(r.time >= before && r.time <= after);
    }

    #[test]
    fn docs_data_map_keys_by_primary_path() {
        let docs = vec![json!({ "id": "a", "n": 1 }), json!({ "id": "b", "n": 2 })];
        let r = RxQuerySingleResult::new("id", docs, 2);
        let m = r.docs_data_map();
        assert_eq!(m.len(), 2);
        assert_eq!(m.get("a").and_then(|d| d.get("n")), Some(&json!(1)));
        assert_eq!(m.get("b").and_then(|d| d.get("n")), Some(&json!(2)));
    }

    #[test]
    fn count_overrides_doc_count_for_count_queries() {
        let r = RxQuerySingleResult::new("id", vec![json!({"id":"x"})], 42);
        assert_eq!(r.count, 42);
        assert_eq!(r.doc_count(), 1);
    }
}
