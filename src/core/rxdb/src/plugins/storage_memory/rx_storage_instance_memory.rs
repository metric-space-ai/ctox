//! Port of `src/plugins/storage-memory/rx-storage-instance-memory.ts`.
//!
//! Functional in-memory storage port for CTOX core. `bulk_write`,
//! `find_documents_by_id`, `query`, `count`, `cleanup`, `change_stream`,
//! `get_changed_documents_since`, `remove`, and `close` are implemented against
//! the Rust query matcher/sort helper stack.
//!
//! T1 deviations:
//! - Lazy `requestIdleCallback`-based persistence (`ensurePersistenceTask`) is
//!   collapsed to synchronous in-place update. CTOX runs server-side so the
//!   IDLE-callback dance is pure overhead.
//! - `OPEN_MEMORY_INSTANCES` global tracking set is omitted (it is a test hook
//!   in upstream).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[allow(unused_imports)]
use async_trait::async_trait;
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::custom_index::get_start_index_string_from_lower_bound;
use crate::plugins::storage_memory::binary_search_bounds::bound_gt;
use crate::plugins::storage_memory::memory_helper::{
    compare_docs_with_index, ensure_not_removed, get_memory_collection_key, remove_doc_from_state,
};
use crate::plugins::storage_memory::memory_indexes::{
    add_indexes_to_internals_state, get_memory_index_name,
};
use crate::plugins::storage_memory::memory_types::{
    DocWithIndexString, MemoryStorageInternals, RxStorageMemoryInstanceCreationOptions,
    RxStorageMemorySettings, SharedMemoryStorageInternals,
};
use crate::plugins::utils::utils_string::random_token;
use crate::plugins::utils::utils_time::now;
use crate::rx_error::{new_rx_error, RxResult};
use crate::rx_schema_helper::get_primary_field_of_primary_key;
use crate::rx_storage_helper::categorize_bulk_write_rows;
use crate::rxjs_compat::{RxStream, RxSubject};
use crate::types::{
    BulkWriteRow, EventBulk, RxJsonSchema, RxStorageBulkWriteResponse,
    RxStorageChangedDocumentsSinceResult, RxStorageCountResult, RxStorageInstance,
    RxStorageInstanceCreationParams, RxStorageQueryResult,
};

// ref: rxdb/src/plugins/storage-memory/rx-storage-instance-memory.ts:71-475
pub struct RxStorageInstanceMemory {
    pub collection_states: Arc<Mutex<HashMap<String, SharedMemoryStorageInternals>>>,
    pub database_name: String,
    pub collection_name: String,
    pub schema: RxJsonSchema,
    pub internals: SharedMemoryStorageInternals,
    pub primary_path: String,
    pub dev_mode: bool,
    pub closed: AtomicBool,
    _settings: RxStorageMemorySettings,
    _options: RxStorageMemoryInstanceCreationOptions,
}

#[async_trait]
impl RxStorageInstance for RxStorageInstanceMemory {
    fn database_name(&self) -> &str {
        &self.database_name
    }
    fn collection_name(&self) -> &str {
        &self.collection_name
    }
    fn schema(&self) -> &RxJsonSchema {
        &self.schema
    }

    // ref: rxdb/src/plugins/storage-memory/rx-storage-instance-memory.ts:103-153
    async fn bulk_write(
        &self,
        document_writes: Vec<BulkWriteRow>,
        context: &str,
    ) -> RxResult<RxStorageBulkWriteResponse> {
        let mut internals = self.internals.lock();
        ensure_not_removed(&internals, &self.database_name, &self.collection_name)?;
        let documents_by_id = internals.documents.clone();
        let categorized = categorize_bulk_write_rows(
            false, // schema_has_attachments — out-of-band in CTOX
            &self.primary_path,
            &documents_by_id,
            &document_writes,
            context,
        );

        // Apply inserts and updates synchronously (T1 deviation: no lazy
        // requestIdleCallback path).
        let mut state_by_index_names: Vec<String> = internals.by_index.keys().cloned().collect();
        for insert in categorized.bulk_insert_docs.iter() {
            let doc = &insert.document;
            let doc_id = doc
                .get(&self.primary_path)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            apply_put(
                &mut internals,
                &mut state_by_index_names,
                &doc_id,
                doc,
                None,
            )?;
        }
        for update in categorized.bulk_update_docs.iter() {
            let doc = &update.document;
            let doc_id = doc
                .get(&self.primary_path)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let prev = documents_by_id.get(&doc_id).cloned();
            apply_put(
                &mut internals,
                &mut state_by_index_names,
                &doc_id,
                doc,
                prev.as_ref(),
            )?;
        }

        // Emit events.
        let mut event_bulk = categorized.event_bulk.clone();
        if !event_bulk.events.is_empty() {
            if let Some(newest) = categorized.newest_row.as_ref() {
                let last_state = &newest.document;
                event_bulk.checkpoint = Some(json!({
                    "id": last_state.get(&self.primary_path).cloned().unwrap_or(Value::Null),
                    "lwt": last_state.get("_meta").and_then(|m| m.get("lwt")).cloned().unwrap_or(Value::Null),
                }));
            }
            internals.changes_subject.next(event_bulk);
        }

        Ok(RxStorageBulkWriteResponse {
            error: categorized.errors,
        })
    }

    // ref: rxdb/src/plugins/storage-memory/rx-storage-instance-memory.ts:242-266
    async fn find_documents_by_id(
        &self,
        doc_ids: &[String],
        with_deleted: bool,
    ) -> RxResult<Vec<Value>> {
        let internals = self.internals.lock();
        let mut ret = Vec::new();
        if internals.documents.is_empty() {
            return Ok(ret);
        }
        for doc_id in doc_ids.iter() {
            if let Some(doc_in_db) = internals.documents.get(doc_id) {
                let deleted = doc_in_db
                    .get("_deleted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !deleted || with_deleted {
                    ret.push(doc_in_db.clone());
                }
            }
        }
        Ok(ret)
    }

    // ref: rxdb/src/plugins/storage-memory/rx-storage-instance-memory.ts:268-365
    async fn query(&self, prepared_query: &Value) -> RxResult<RxStorageQueryResult> {
        use crate::plugins::storage_memory::binary_search_bounds::{bound_le, bound_lt};
        use crate::plugins::storage_memory::memory_types::DocWithIndexString;
        use crate::rx_query_helper::{get_query_matcher, get_sort_comparator};
        use crate::types::{FilledMangoQuery, RxQueryPlan};

        let query_plan: RxQueryPlan = serde_json::from_value(
            prepared_query
                .get("queryPlan")
                .cloned()
                .unwrap_or(Value::Null),
        )
        .map_err(|e| {
            new_rx_error(
                "STO20",
                Some(json!({ "message": format!("invalid prepared_query.queryPlan: {e}") })),
            )
        })?;
        let query: FilledMangoQuery =
            serde_json::from_value(prepared_query.get("query").cloned().unwrap_or(Value::Null))
                .map_err(|e| {
                    new_rx_error(
                        "STO20",
                        Some(json!({ "message": format!("invalid prepared_query.query: {e}") })),
                    )
                })?;

        let skip = query.skip.unwrap_or(0) as usize;
        let limit = query.limit.map(|l| l as usize).unwrap_or(usize::MAX);
        let skip_plus_limit = skip.saturating_add(limit);

        let query_matcher = if !query_plan.selector_satisfied_by_index {
            Some(get_query_matcher(&self.schema, &query))
        } else {
            None
        };

        let must_manually_resort = !query_plan.sort_satisfied_by_index;
        let index = &query_plan.index;
        let lower_bound_string =
            get_start_index_string_from_lower_bound(&self.schema, index, &query_plan.start_keys)?;
        let upper_bound_string = crate::custom_index::get_start_index_string_from_upper_bound(
            &self.schema,
            index,
            &query_plan.end_keys,
        )?;
        let index_name = get_memory_index_name(index);

        // Snapshot the docs_with_index slice under the lock, then release.
        let docs_with_index: Vec<DocWithIndexString> = {
            let internals = self.internals.lock();
            let by_index = internals.by_index.get(&index_name).ok_or_else(|| {
                new_rx_error(
                    "STO21",
                    Some(json!({
                        "message": format!("memory index does not exist: {index_name}")
                    })),
                )
            })?;
            by_index.docs_with_index.clone()
        };

        let lower_probe = DocWithIndexString {
            index_string: lower_bound_string,
            document: Value::Null,
            id: String::new(),
        };
        let upper_probe = DocWithIndexString {
            index_string: upper_bound_string,
            document: Value::Null,
            id: String::new(),
        };
        let mut index_of_lower = if query_plan.inclusive_start {
            crate::plugins::storage_memory::binary_search_bounds::bound_ge(
                &docs_with_index,
                &lower_probe,
                &compare_docs_with_index,
                None,
                None,
            )
        } else {
            bound_gt(
                &docs_with_index,
                &lower_probe,
                &compare_docs_with_index,
                None,
                None,
            )
        };
        let index_of_upper = if query_plan.inclusive_end {
            bound_le(
                &docs_with_index,
                &upper_probe,
                &compare_docs_with_index,
                None,
                None,
            )
        } else {
            bound_lt(
                &docs_with_index,
                &upper_probe,
                &compare_docs_with_index,
                None,
                None,
            )
        };

        let mut rows: Vec<Value> = Vec::new();
        while index_of_lower <= index_of_upper && (index_of_lower as usize) < docs_with_index.len()
        {
            let current = &docs_with_index[index_of_lower as usize];
            let doc = &current.document;
            let matches = match &query_matcher {
                Some(m) => m(doc),
                None => true,
            };
            if matches {
                rows.push(doc.clone());
            }
            if rows.len() >= skip_plus_limit && !must_manually_resort {
                break;
            }
            index_of_lower += 1;
        }

        if must_manually_resort {
            let cmp = get_sort_comparator(&self.schema, &query);
            rows.sort_by(|a, b| cmp(a, b));
        }

        // Apply skip + limit.
        let len = rows.len();
        let start = skip.min(len);
        let end = skip_plus_limit.min(len);
        rows = rows[start..end].to_vec();

        Ok(RxStorageQueryResult { documents: rows })
    }

    async fn count(&self, prepared_query: &Value) -> RxResult<RxStorageCountResult> {
        let result = self.query(prepared_query).await?;
        Ok(RxStorageCountResult {
            count: result.documents.len() as u64,
            mode: "fast".to_string(),
        })
    }

    async fn get_changed_documents_since(
        &self,
        limit: u64,
        checkpoint: Option<&Value>,
    ) -> RxResult<RxStorageChangedDocumentsSinceResult> {
        let since_lwt = checkpoint
            .and_then(|checkpoint| checkpoint.get("lwt"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let since_id = checkpoint
            .and_then(|checkpoint| checkpoint.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let mut documents: Vec<Value> = {
            let internals = self.internals.lock();
            ensure_not_removed(&internals, &self.database_name, &self.collection_name)?;
            internals
                .documents
                .values()
                .filter(|doc| {
                    let lwt = doc
                        .get("_meta")
                        .and_then(|meta| meta.get("lwt"))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    let id = doc
                        .get(&self.primary_path)
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    lwt > since_lwt || (lwt == since_lwt && id > since_id.as_str())
                })
                .cloned()
                .collect()
        };
        documents.sort_by(|left, right| {
            let left_lwt = left
                .get("_meta")
                .and_then(|meta| meta.get("lwt"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let right_lwt = right
                .get("_meta")
                .and_then(|meta| meta.get("lwt"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            left_lwt
                .partial_cmp(&right_lwt)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let left_id = left
                        .get(&self.primary_path)
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let right_id = right
                        .get(&self.primary_path)
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    left_id.cmp(right_id)
                })
        });
        documents.truncate(limit as usize);

        let checkpoint = documents
            .last()
            .map(|doc| {
                json!({
                    "id": doc.get(&self.primary_path).cloned().unwrap_or(Value::Null),
                    "lwt": doc
                        .get("_meta")
                        .and_then(|meta| meta.get("lwt"))
                        .cloned()
                        .unwrap_or(json!(0)),
                })
            })
            .or_else(|| checkpoint.cloned())
            .unwrap_or_else(|| json!({ "id": "", "lwt": 0 }));

        Ok(RxStorageChangedDocumentsSinceResult {
            documents,
            checkpoint,
        })
    }

    fn change_stream(&self) -> RxStream<EventBulk> {
        let internals = self.internals.lock();
        internals.changes_subject.subscribe()
    }

    // ref: rxdb/src/plugins/storage-memory/rx-storage-instance-memory.ts:378-419
    async fn cleanup(&self, minimum_deleted_time: i64) -> RxResult<bool> {
        let mut internals = self.internals.lock();
        let max_deletion_time = (now() as i64) - minimum_deleted_time;
        let index = vec![
            "_deleted".to_string(),
            "_meta.lwt".to_string(),
            self.primary_path.clone(),
        ];
        let index_name = get_memory_index_name(&index);
        // Collect ids to remove first to avoid borrowing conflicts.
        let to_remove: Vec<Value> = {
            let by_index = match internals.by_index.get(&index_name) {
                Some(b) => b,
                None => return Ok(true),
            };
            let lower_bound_string = get_start_index_string_from_lower_bound(
                &self.schema,
                &index,
                &[Value::Bool(true), json!(0), Value::String(String::new())],
            )?;
            let probe = DocWithIndexString {
                index_string: lower_bound_string,
                document: Value::Null,
                id: String::new(),
            };
            let mut index_of_lower = bound_gt(
                &by_index.docs_with_index,
                &probe,
                &compare_docs_with_index,
                None,
                None,
            );
            let mut out = Vec::new();
            while let Some(current_doc) = by_index.docs_with_index.get(index_of_lower as usize) {
                let lwt = current_doc
                    .document
                    .get("_meta")
                    .and_then(|m| m.get("lwt"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if lwt > max_deletion_time {
                    break;
                }
                out.push(current_doc.document.clone());
                index_of_lower += 1;
            }
            out
        };
        for doc in to_remove.iter() {
            remove_doc_from_state(&self.primary_path, &self.schema, &mut internals, doc);
        }
        Ok(true)
    }

    // ref: rxdb/src/plugins/storage-memory/rx-storage-instance-memory.ts:446-462
    async fn remove(&self) -> RxResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(new_rx_error(
                "STO17",
                Some(json!({ "message": "instance already closed" })),
            ));
        }
        {
            let mut internals = self.internals.lock();
            ensure_not_removed(&internals, &self.database_name, &self.collection_name)?;
            internals.removed = true;
        }
        let key = get_memory_collection_key(
            &self.database_name,
            &self.collection_name,
            self.schema.version,
        );
        self.collection_states.lock().remove(&key);
        self.do_close();
        Ok(())
    }

    async fn close(&self) -> RxResult<()> {
        self.do_close();
        Ok(())
    }
}

impl RxStorageInstanceMemory {
    fn do_close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut internals = self.internals.lock();
        internals.ref_count = internals.ref_count.saturating_sub(1);
    }

    // Used to keep a strongly-typed handle in CTOX glue.
    fn _zero_use(&self) {
        let _ = (&self._settings, &self._options);
    }
}

/// Hot-path put helper that bridges into the mutable borrows we need.
fn apply_put(
    internals: &mut MemoryStorageInternals,
    state_by_index_names: &mut [String],
    doc_id: &str,
    doc: &Value,
    doc_in_state: Option<&Value>,
) -> RxResult<()> {
    // We need to call put_write_row_to_state with mutable refs into each
    // by_index. To avoid borrow conflicts with `internals`, we collect mutable
    // refs in a fresh Vec scoped to this call.
    let mut by_index_refs: Vec<
        &mut crate::plugins::storage_memory::memory_types::MemoryStorageInternalsByIndex,
    > = Vec::with_capacity(state_by_index_names.len());
    // SAFETY: each name maps to a distinct entry; we collect non-overlapping
    // mutable references. We do this by removing+re-inserting via an
    // intermediate take pattern: collect raw pointers, sort/dedup-check by
    // identity, then build the &mut refs in a single scope. The simpler way
    // is `iter_mut().filter(...)` over the values:
    for by_index in internals.by_index.values_mut() {
        by_index_refs.push(by_index);
    }
    // Update documents map *after* establishing index views — but
    // put_write_row_to_state needs to mutate state.documents internally. To
    // keep the borrow checker happy we do the documents insert inline first.
    internals.documents.insert(doc_id.to_string(), doc.clone());
    // Now re-take by-index refs without `internals.documents` aliasing.
    let mut by_index_refs2: Vec<
        &mut crate::plugins::storage_memory::memory_types::MemoryStorageInternalsByIndex,
    > = internals.by_index.values_mut().collect();
    put_write_row_to_state_no_docs(doc_id, &mut by_index_refs2, doc, doc_in_state)?;
    Ok(())
}

/// Same as `memory_helper::put_write_row_to_state` minus the `state.documents`
/// write (caller handles it). Lets us split mutable borrows across the
/// `documents` map and the `by_index` map.
fn put_write_row_to_state_no_docs(
    doc_id: &str,
    state_by_index: &mut [&mut crate::plugins::storage_memory::memory_types::MemoryStorageInternalsByIndex],
    document: &Value,
    doc_in_state: Option<&Value>,
) -> RxResult<()> {
    use crate::plugins::storage_memory::binary_search_bounds::bound_eq;
    use crate::plugins::storage_memory::memory_types::DocWithIndexString;
    use std::cmp::Ordering as Ord;

    for by_index in state_by_index.iter_mut() {
        let new_index_string = (by_index.get_indexable_string)(document);
        // Insert at sorted position via binary_search_by + insert.
        let rel = by_index
            .docs_with_index
            .binary_search_by(|x| {
                if x.index_string < new_index_string {
                    Ord::Less
                } else {
                    Ord::Greater
                }
            })
            .unwrap_or_else(|e| e);
        by_index.docs_with_index.insert(
            rel,
            DocWithIndexString {
                index_string: new_index_string.clone(),
                document: document.clone(),
                id: doc_id.to_string(),
            },
        );

        if let Some(prev_doc) = doc_in_state {
            let previous_index_string = (by_index.get_indexable_string)(prev_doc);
            if previous_index_string == new_index_string {
                let docs = &mut by_index.docs_with_index;
                if rel > 0 && docs.get(rel - 1).map(|d| d.id.as_str()) == Some(doc_id) {
                    docs.remove(rel - 1);
                    continue;
                }
                if docs.get(rel + 1).map(|d| d.id.as_str()) == Some(doc_id) {
                    docs.remove(rel + 1);
                    continue;
                }
                return Err(new_rx_error("SNH", Some(json!({ "document": document }))));
            } else {
                let probe = DocWithIndexString {
                    index_string: previous_index_string,
                    document: Value::Null,
                    id: String::new(),
                };
                let pos = bound_eq(
                    &by_index.docs_with_index,
                    &probe,
                    &(|a: &DocWithIndexString, b: &DocWithIndexString| {
                        if a.index_string < b.index_string {
                            Ord::Less
                        } else if a.index_string == b.index_string {
                            Ord::Equal
                        } else {
                            Ord::Greater
                        }
                    }),
                    None,
                    None,
                );
                if pos >= 0 {
                    by_index.docs_with_index.remove(pos as usize);
                }
            }
        }
    }
    Ok(())
}

/// Storage factory holding shared internals across instances.
pub struct RxStorageMemory {
    pub name: String,
    pub collection_states: Arc<Mutex<HashMap<String, SharedMemoryStorageInternals>>>,
}

impl RxStorageMemory {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            name: "memory".to_string(),
            collection_states: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    // ref: rxdb/src/plugins/storage-memory/rx-storage-instance-memory.ts:478-531
    pub async fn create_storage_instance(
        &self,
        params: RxStorageInstanceCreationParams,
        settings: RxStorageMemorySettings,
    ) -> RxResult<Arc<RxStorageInstanceMemory>> {
        let collection_key = get_memory_collection_key(
            &params.database_name,
            &params.collection_name,
            params.schema.version,
        );

        let internals = {
            let mut map = self.collection_states.lock();
            if let Some(existing) = map.get(&collection_key).cloned() {
                // Schema-equality check (upstream uses devMode + deepEqual).
                if params.dev_mode {
                    let existing_schema = existing.lock().schema.clone();
                    if serde_json::to_value(&existing_schema).ok()
                        != serde_json::to_value(&params.schema).ok()
                    {
                        return Err(new_rx_error(
                            "STO18",
                            Some(json!({
                                "message": "storage was already created with a different schema",
                            })),
                        ));
                    }
                }
                existing.lock().ref_count += 1;
                existing
            } else {
                let mut state = MemoryStorageInternals {
                    id: random_token(Some(5)),
                    schema: params.schema.clone(),
                    removed: false,
                    ref_count: 1,
                    documents: HashMap::new(),
                    attachments: HashMap::new(),
                    by_index: HashMap::new(),
                    changes_subject: RxSubject::new(),
                };
                add_indexes_to_internals_state(&mut state, &params.schema)?;
                let shared: SharedMemoryStorageInternals = Arc::new(Mutex::new(state));
                map.insert(collection_key.clone(), Arc::clone(&shared));
                shared
            }
        };

        let primary_path = get_primary_field_of_primary_key(&params.schema.primary_key);
        Ok(Arc::new(RxStorageInstanceMemory {
            collection_states: Arc::clone(&self.collection_states),
            database_name: params.database_name,
            collection_name: params.collection_name,
            schema: params.schema,
            internals,
            primary_path,
            dev_mode: params.dev_mode,
            closed: AtomicBool::new(false),
            _settings: settings,
            _options: (),
        }))
    }
}

#[async_trait]
impl crate::types::RxStorage for RxStorageMemory {
    fn name(&self) -> &str {
        &self.name
    }
    async fn create_storage_instance(
        &self,
        params: RxStorageInstanceCreationParams,
    ) -> RxResult<Arc<dyn crate::types::RxStorageInstance>> {
        let instance = self.create_storage_instance(params, ()).await?;
        Ok(instance as Arc<dyn crate::types::RxStorageInstance>)
    }
}
