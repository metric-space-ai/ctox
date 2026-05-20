//! Rust-native port of `src/rx-query.ts`.
//!
//! This file keeps the upstream query data flow intact: normalize Mango query,
//! add the `_deleted = false` storage filter, prepare the query, execute
//! against the collection storage instance and cache the last
//! [`crate::rx_query_single_result::RxQuerySingleResult`]. RxJS-style
//! reactivity is exposed through typed Rust streams and `RxBehaviorSubject`
//! adapters; event-reduce is implemented as a conservative Rust subset that
//! patches unpaginated find results and falls back to storage re-execution for
//! ambiguous or paginated cases.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use futures::{stream, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::event_reduce::{calculate_new_results, calculate_patched_find_results};
use crate::hooks::run_plugin_hooks;
use crate::plugins::utils::utils_error::plugin_missing;
use crate::plugins::utils::utils_time::now;
use crate::rx_collection::RxCollection;
use crate::rx_document::RxDocument;
use crate::rx_error::{new_rx_error, RxResult};
use crate::rx_query_helper::{
    get_query_matcher, get_sort_comparator, normalize_mango_query, prepare_query,
};
use crate::rx_query_single_result::RxQuerySingleResult;
use crate::rx_storage_helper::throw_if_is_storage_write_error;
use crate::rxjs_compat::{reactive_from_stream, RxBehaviorSubject, RxStream};
use crate::types::{FilledMangoQuery, MangoQuery, PreparedQuery};

static QUERY_COUNT: AtomicU64 = AtomicU64::new(0);

fn new_query_id() -> u64 {
    QUERY_COUNT.fetch_add(1, Ordering::SeqCst) + 1
}

// ref: rxdb/src/types/rx-query.d.ts RxQueryOP
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RxQueryOp {
    Find,
    FindOne,
    FindByIds,
    Count,
}

impl RxQueryOp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Find => "find",
            Self::FindOne => "findOne",
            Self::FindByIds => "findByIds",
            Self::Count => "count",
        }
    }
}

pub struct RxQueryBase {
    pub id: u64,
    pub op: RxQueryOp,
    pub mango_query: MangoQuery,
    pub collection: Arc<RxCollection>,
    pub other: Value,
    pub exec_over_database_count: Mutex<u64>,
    pub creation_time: f64,
    pub last_ensure_equal: Mutex<f64>,
    pub uncached: AtomicBool,
    pub is_find_one_by_id_query: Option<Vec<String>>,
    latest_change_event: Mutex<Option<u64>>,
    result: Mutex<Option<RxQuerySingleResult>>,
    prepared_query: Mutex<Option<PreparedQuery>>,
}

impl RxQueryBase {
    // ref: rxdb/src/rx-query.ts constructor
    pub fn new(
        op: RxQueryOp,
        mango_query: Option<MangoQuery>,
        collection: Arc<RxCollection>,
        other: Option<Value>,
    ) -> Self {
        let mango_query = mango_query.unwrap_or_else(get_default_query);
        let primary_path = collection
            .primary_path()
            .unwrap_or_else(|| "id".to_string());
        let is_find_one_by_id_query = is_find_one_by_id_query(&primary_path, &mango_query);
        Self {
            id: new_query_id(),
            op,
            mango_query,
            collection,
            other: other.unwrap_or(Value::Null),
            exec_over_database_count: Mutex::new(0),
            creation_time: now(),
            last_ensure_equal: Mutex::new(0.0),
            uncached: AtomicBool::new(false),
            is_find_one_by_id_query,
            latest_change_event: Mutex::new(None),
            result: Mutex::new(None),
            prepared_query: Mutex::new(None),
        }
    }

    // ref: rxdb/src/rx-query.ts _setResultData
    pub fn set_result_data(&self, new_result_data: QueryExecutionResult) -> RxResult<()> {
        let result = match new_result_data {
            QueryExecutionResult::Count(count) => {
                RxQuerySingleResult::from_collection(&self.collection, Vec::new(), count)?
            }
            QueryExecutionResult::Documents(docs) => RxQuerySingleResult::from_collection(
                &self.collection,
                docs.clone(),
                docs.len() as u64,
            )?,
            QueryExecutionResult::DocumentsById(map) => {
                let docs: Vec<Value> = map.into_values().collect();
                RxQuerySingleResult::from_collection(
                    &self.collection,
                    docs.clone(),
                    docs.len() as u64,
                )?
            }
        };
        *self.result.lock() = Some(result);
        Ok(())
    }

    pub(crate) fn mark_dirty(&self) {
        *self.latest_change_event.lock() = None;
    }

    pub(crate) fn mark_uncached(&self) {
        self.uncached.store(true, Ordering::SeqCst);
    }

    pub fn is_uncached(&self) -> bool {
        self.uncached.load(Ordering::SeqCst)
    }

    pub(crate) fn subscriber_count(&self) -> usize {
        0
    }

    // ref: rxdb/src/rx-query.ts _execOverDatabase
    pub async fn exec_over_database(&self) -> RxResult<QueryExecutionResult> {
        *self.exec_over_database_count.lock() += 1;
        match self.op {
            RxQueryOp::Count => {
                let prepared = self.get_prepared_query()?;
                let result = self.collection.storage_instance.count(&prepared).await?;
                if result.mode == "slow" && !self.collection.database.allow_slow_count {
                    return Err(new_rx_error(
                        "QU14",
                        Some(json!({
                            "collection": self.collection.name,
                            "queryObj": self.mango_query,
                        })),
                    ));
                }
                Ok(QueryExecutionResult::Count(result.count))
            }
            RxQueryOp::FindByIds => {
                let primary_path = self.collection.primary_path().ok_or_else(|| {
                    new_rx_error(
                        "QU_SCHEMA",
                        Some(json!({ "collection": self.collection.name })),
                    )
                })?;
                let ids = find_by_ids_ids(&self.mango_query, &primary_path)?;
                let docs = self
                    .collection
                    .storage_instance
                    .find_documents_by_id(&ids, false)
                    .await?;
                let mut ret = HashMap::new();
                for doc in docs {
                    if let Some(id) = doc.get(&primary_path).and_then(Value::as_str) {
                        ret.insert(id.to_string(), doc);
                    }
                }
                Ok(QueryExecutionResult::DocumentsById(ret))
            }
            RxQueryOp::Find | RxQueryOp::FindOne => {
                let docs = query_collection(self).await?;
                Ok(QueryExecutionResult::Documents(docs))
            }
        }
    }

    // ref: rxdb/src/rx-query.ts exec
    pub async fn exec(&self, throw_if_missing: bool) -> RxResult<Value> {
        if throw_if_missing && self.op != RxQueryOp::FindOne {
            return Err(new_rx_error(
                "QU9",
                Some(json!({
                    "collection": self.collection.name,
                    "op": self.op.as_str(),
                })),
            ));
        }
        self.ensure_equal().await?;
        let query_key = self.to_string_key()?;
        let result_guard = self.result.lock();
        let result = result_guard
            .as_ref()
            .ok_or_else(|| new_rx_error("QU_RESULT", Some(json!({ "query": query_key }))))?;
        self.result_value(result, throw_if_missing)
    }

    pub async fn exec_rx_documents(&self, throw_if_missing: bool) -> RxResult<RxQueryExecResult> {
        if throw_if_missing && self.op != RxQueryOp::FindOne {
            return Err(new_rx_error(
                "QU9",
                Some(json!({
                    "collection": self.collection.name,
                    "op": self.op.as_str(),
                })),
            ));
        }
        self.ensure_equal().await?;
        let query_key = self.to_string_key()?;
        let result_guard = self.result.lock();
        let result = result_guard
            .as_ref()
            .ok_or_else(|| new_rx_error("QU_RESULT", Some(json!({ "query": query_key }))))?;
        self.rx_document_result_value(result, throw_if_missing)
    }

    // ref: rxdb/src/rx-query.ts `$`
    pub fn result_stream(self: &Arc<Self>) -> RxResult<RxStream<RxQueryExecResult>> {
        let query = Arc::clone(self);
        let initial = stream::once(async move { query.exec_rx_documents(false).await.ok() });
        let query = Arc::clone(self);
        let changes = self.collection.event_bulks().filter_map(move |bulk| {
            let query = Arc::clone(&query);
            async move {
                let _ = bulk;
                let changed = query.ensure_equal().await.ok()?;
                if !changed {
                    return None;
                }
                let result_guard = query.result.lock();
                let result = result_guard.as_ref()?;
                query.rx_document_result_value(result, false).ok()
            }
        });
        Ok(Box::pin(
            initial
                .chain(changes.map(Some))
                .filter_map(|result| async move { result }),
        ))
    }

    // ref: rxdb/src/rx-query.ts:153-160 $$
    pub fn double_dollar(
        self: &Arc<Self>,
    ) -> RxResult<RxBehaviorSubject<Option<RxQueryExecResult>>> {
        Ok(reactive_from_stream(
            None,
            Box::pin(self.result_stream()?.map(Some)),
        ))
    }

    pub async fn ensure_equal(&self) -> RxResult<bool> {
        *self.last_ensure_equal.lock() = now();
        self.collection.await_before_reads().await?;
        if self.result.lock().is_some() {
            if let Some(buffer) = self.collection.change_event_buffer.as_ref() {
                let current_counter = buffer.get_counter();
                if self
                    .latest_change_event
                    .lock()
                    .is_some_and(|latest| latest >= current_counter)
                {
                    return Ok(false);
                }
            }
        }
        if let Some(changed) = self.try_incremental_count_update()? {
            return Ok(changed);
        }
        if let Some(changed) = self.try_event_reduce_noop_update()? {
            return Ok(changed);
        }
        let previous_time = self.result.lock().as_ref().map(|result| result.time);
        let new_data = self.exec_over_database().await?;
        self.set_result_data(new_data)?;
        if let Some(buffer) = self.collection.change_event_buffer.as_ref() {
            *self.latest_change_event.lock() = Some(buffer.get_counter());
        }
        let current_time = self.result.lock().as_ref().map(|result| result.time);
        Ok(previous_time != current_time)
    }

    fn try_incremental_count_update(&self) -> RxResult<Option<bool>> {
        if self.op != RxQueryOp::Count {
            return Ok(None);
        }
        let Some(buffer) = self.collection.change_event_buffer.as_ref() else {
            return Ok(None);
        };
        let Some(latest_change_event) = *self.latest_change_event.lock() else {
            return Ok(None);
        };
        let current_counter = buffer.get_counter();
        if latest_change_event >= current_counter {
            return Ok(Some(false));
        }
        let Some(missed_change_events) = buffer.get_from(latest_change_event + 1) else {
            return Ok(None);
        };
        let Some(previous_count) = self.result.lock().as_ref().map(|result| result.count) else {
            return Ok(None);
        };

        let mut new_count = previous_count as i64;
        for event in buffer.reduce_by_last_of_doc(&missed_change_events) {
            let did_match_before = event
                .previous_document_data
                .as_ref()
                .map(|doc| self.does_document_data_match(doc))
                .transpose()?
                .unwrap_or(false);
            let does_match_now = event
                .document_data
                .as_ref()
                .map(|doc| self.does_document_data_match(doc))
                .transpose()?
                .unwrap_or(false);

            if !did_match_before && does_match_now {
                new_count += 1;
            }
            if did_match_before && !does_match_now {
                new_count -= 1;
            }
        }

        *self.latest_change_event.lock() = Some(current_counter);
        let new_count = new_count.max(0) as u64;
        if new_count != previous_count {
            self.set_result_data(QueryExecutionResult::Count(new_count))?;
            Ok(Some(true))
        } else {
            Ok(Some(false))
        }
    }

    fn try_event_reduce_noop_update(&self) -> RxResult<Option<bool>> {
        if !matches!(self.op, RxQueryOp::Find | RxQueryOp::FindOne) {
            return Ok(None);
        }
        if !self.collection.database.event_reduce {
            return Ok(None);
        }
        let Some(buffer) = self.collection.change_event_buffer.as_ref() else {
            return Ok(None);
        };
        let Some(latest_change_event) = *self.latest_change_event.lock() else {
            return Ok(None);
        };
        let current_counter = buffer.get_counter();
        if latest_change_event >= current_counter {
            return Ok(Some(false));
        }
        let Some(missed_change_events) = buffer.get_from(latest_change_event + 1) else {
            return Ok(None);
        };
        let previous_results = self
            .result
            .lock()
            .as_ref()
            .map(|result| result.docs_data().to_vec())
            .unwrap_or_default();
        let reduced_events = buffer.reduce_by_last_of_doc(&missed_change_events);
        let result = if self.op == RxQueryOp::Find {
            if let Some((primary_path, sort_comparator)) = self.event_reduce_patch_context()? {
                calculate_patched_find_results(
                    &previous_results,
                    &reduced_events,
                    |doc| self.does_document_data_match(doc),
                    &primary_path,
                    |a, b| sort_comparator(a, b),
                )?
            } else {
                calculate_new_results(&previous_results, &reduced_events, |doc| {
                    self.does_document_data_match(doc)
                })?
            }
        } else {
            calculate_new_results(&previous_results, &reduced_events, |doc| {
                self.does_document_data_match(doc)
            })?
        };
        if result.run_full_query_again {
            return Ok(None);
        }
        *self.latest_change_event.lock() = Some(current_counter);
        if result.changed {
            self.set_result_data(QueryExecutionResult::Documents(result.new_results))?;
        }
        Ok(Some(result.changed))
    }

    fn event_reduce_patch_context(
        &self,
    ) -> RxResult<Option<(String, crate::rx_query_helper::DeterministicSortComparator)>> {
        let Some(schema) = self.collection.schema.as_ref() else {
            return Ok(None);
        };
        let normalized = normalize_mango_query(&schema.json_schema, self.mango_query.clone());
        if normalized.skip.unwrap_or(0) != 0 || normalized.limit.is_some() {
            return Ok(None);
        }
        let primary_path = self.collection.primary_path().ok_or_else(|| {
            new_rx_error(
                "QU_SCHEMA",
                Some(json!({ "collection": self.collection.name })),
            )
        })?;
        let sort_comparator = get_sort_comparator(&schema.json_schema, &normalized);
        Ok(Some((primary_path, sort_comparator)))
    }

    // ref: rxdb/src/rx-query.ts getPreparedQuery
    pub fn get_prepared_query(&self) -> RxResult<PreparedQuery> {
        if let Some(existing) = self.prepared_query.lock().clone() {
            return Ok(existing);
        }
        let schema = self.collection.schema.as_ref().ok_or_else(|| {
            new_rx_error(
                "QU_SCHEMA",
                Some(json!({ "collection": self.collection.name })),
            )
        })?;
        let mut mango_query = normalize_mango_query(&schema.json_schema, self.mango_query.clone());
        add_deleted_false_selector(&mut mango_query);
        if let Some(index) = mango_query.index.as_mut() {
            index.insert(0, "_deleted".to_string());
        }
        let mut hook_input = json!({
            "rxQuery": {
                "id": self.id,
                "op": self.op.as_str(),
                "collection": self.collection.name
            },
            "mangoQuery": mango_query
        });
        run_plugin_hooks("prePrepareQuery", &mut hook_input);
        let mango_query: FilledMangoQuery = hook_input
            .get("mangoQuery")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or(mango_query);
        let prepared = prepare_query(&schema.json_schema, mango_query)?;
        *self.prepared_query.lock() = Some(prepared.clone());
        Ok(prepared)
    }

    pub fn query_matcher(&self) -> RxResult<crate::rx_query_helper::QueryMatcher> {
        let schema = self.collection.schema.as_ref().ok_or_else(|| {
            new_rx_error(
                "QU_SCHEMA",
                Some(json!({ "collection": self.collection.name })),
            )
        })?;
        let normalized = normalize_mango_query(&schema.json_schema, self.mango_query.clone());
        Ok(get_query_matcher(&schema.json_schema, &normalized))
    }

    // ref: rxdb/src/rx-query.ts doesDocumentDataMatch
    pub fn does_document_data_match(&self, doc_data: &Value) -> RxResult<bool> {
        if doc_data
            .get("_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(false);
        }
        Ok((self.query_matcher()?).as_ref()(doc_data))
    }

    // ref: rxdb/src/rx-query.ts toString
    pub fn to_string_key(&self) -> RxResult<String> {
        let schema = self.collection.schema.as_ref().ok_or_else(|| {
            new_rx_error(
                "QU_SCHEMA",
                Some(json!({ "collection": self.collection.name })),
            )
        })?;
        let normalized = normalize_mango_query(&schema.json_schema, self.mango_query.clone());
        serde_json::to_string(&json!({
            "op": self.op.as_str(),
            "query": normalized,
            "other": self.other,
        }))
        .map_err(|err| new_rx_error("QU_STRING", Some(json!({ "message": err.to_string() }))))
    }

    fn result_value(
        &self,
        result: &RxQuerySingleResult,
        throw_if_missing: bool,
    ) -> RxResult<Value> {
        match self.op {
            RxQueryOp::Count => Ok(json!(result.count)),
            RxQueryOp::Find => Ok(Value::Array(result.docs_data().to_vec())),
            RxQueryOp::FindByIds => Ok(Value::Object(
                result
                    .docs_data_map()
                    .into_iter()
                    .collect::<serde_json::Map<String, Value>>(),
            )),
            RxQueryOp::FindOne => {
                if let Some(doc) = result.docs_data().first() {
                    Ok(doc.clone())
                } else if throw_if_missing {
                    Err(new_rx_error(
                        "QU10",
                        Some(json!({
                            "collection": self.collection.name,
                            "query": self.mango_query,
                        })),
                    ))
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    fn rx_document_result_value(
        &self,
        result: &RxQuerySingleResult,
        throw_if_missing: bool,
    ) -> RxResult<RxQueryExecResult> {
        match self.op {
            RxQueryOp::Count => Ok(RxQueryExecResult::Count(result.count)),
            RxQueryOp::Find => Ok(RxQueryExecResult::Documents(result.documents()?.to_vec())),
            RxQueryOp::FindByIds => Ok(RxQueryExecResult::DocumentsById(result.docs_map()?)),
            RxQueryOp::FindOne => {
                let document = result.documents()?.first().cloned();
                if document.is_none() && throw_if_missing {
                    Err(new_rx_error(
                        "QU10",
                        Some(json!({
                            "collection": self.collection.name,
                            "query": self.mango_query,
                        })),
                    ))
                } else {
                    Ok(RxQueryExecResult::Document(document))
                }
            }
        }
    }

    // ref: rxdb/src/rx-query.ts remove
    pub async fn remove(&self) -> RxResult<Value> {
        let current = self.exec(false).await?;
        let primary_path = self.collection.primary_path().ok_or_else(|| {
            new_rx_error(
                "QU_SCHEMA",
                Some(json!({ "collection": self.collection.name })),
            )
        })?;
        let ids = collect_document_ids(&current, self.op, &primary_path);
        if ids.is_empty() {
            return Ok(match self.op {
                RxQueryOp::FindOne => Value::Null,
                _ => Value::Array(Vec::new()),
            });
        }

        let result = self.collection.bulk_remove_by_ids(ids).await?;
        if let Some(error) = result.error.first() {
            return Err(new_rx_error(
                "QU_REMOVE_ERROR",
                Some(json!({ "writeError": error })),
            ));
        }
        let mut removed_by_id = HashMap::new();
        for document in result.success {
            let json = document.to_json(true);
            if let Some(id) = json.get(&primary_path).and_then(Value::as_str) {
                removed_by_id.insert(id.to_string(), json);
            }
        }
        self.collection.invalidate_query_cache();
        project_documents_like_query_result(self.op, current, &primary_path, &removed_by_id)
    }

    // ref: rxdb/src/rx-query.ts incrementalRemove
    pub async fn incremental_remove(&self) -> RxResult<Value> {
        self.remove().await
    }

    // ref: rxdb/src/rx-query.ts update
    pub fn update(&self, _update_obj: Value) -> RxResult<Value> {
        Err(plugin_missing_rx_error("update"))
    }

    pub async fn patch(&self, patch: Value) -> RxResult<Value> {
        self.modify(move |mut document| {
            let patch = patch.clone();
            async move {
                if let (Some(document_obj), Some(patch_obj)) =
                    (document.as_object_mut(), patch.as_object())
                {
                    for (key, value) in patch_obj {
                        document_obj.insert(key.clone(), value.clone());
                    }
                }
                Ok(document)
            }
        })
        .await
    }

    pub async fn incremental_patch(&self, patch: Value) -> RxResult<Value> {
        self.patch(patch).await
    }

    pub async fn modify<F, Fut>(&self, mut mutation_function: F) -> RxResult<Value>
    where
        F: FnMut(Value) -> Fut,
        Fut: Future<Output = RxResult<Value>>,
    {
        let current = self.exec(false).await?;
        let primary_path = self.collection.primary_path().ok_or_else(|| {
            new_rx_error(
                "QU_SCHEMA",
                Some(json!({ "collection": self.collection.name })),
            )
        })?;
        let documents = collect_documents(&current, self.op);
        if documents.is_empty() {
            return Ok(match self.op {
                RxQueryOp::FindOne => Value::Null,
                _ => Value::Array(Vec::new()),
            });
        }

        let mut write_data = Vec::with_capacity(documents.len());
        for document in documents {
            write_data.push(mutation_function(document).await?);
        }

        let result = self.collection.bulk_upsert(write_data.clone()).await?;
        if let Some(error) = result.error.first() {
            let failed_doc = write_data
                .iter()
                .find(|doc| {
                    doc.get(&primary_path).and_then(Value::as_str)
                        == Some(error.document_id.as_str())
                })
                .unwrap_or(&Value::Null);
            throw_if_is_storage_write_error(
                &self.collection.name,
                &error.document_id,
                failed_doc,
                Some(error),
            )?;
        }

        let mut updated_by_id = HashMap::new();
        for document in result.success {
            let json = document.to_json(true);
            if let Some(id) = json.get(&primary_path).and_then(Value::as_str) {
                updated_by_id.insert(id.to_string(), json);
            }
        }
        self.collection.invalidate_query_cache();
        project_documents_like_query_result(self.op, current, &primary_path, &updated_by_id)
    }

    pub async fn incremental_modify<F, Fut>(&self, mutation_function: F) -> RxResult<Value>
    where
        F: FnMut(Value) -> Fut,
        Fut: Future<Output = RxResult<Value>>,
    {
        self.modify(mutation_function).await
    }

    // ref: rxdb/src/rx-query.ts query-builder methods
    pub fn where_(&self, _query_obj: Value) -> RxResult<Arc<RxQueryBase>> {
        Err(plugin_missing_rx_error("query-builder"))
    }

    pub fn sort(&self, _params: Value) -> RxResult<Arc<RxQueryBase>> {
        Err(plugin_missing_rx_error("query-builder"))
    }

    pub fn skip(&self, _amount: Option<u64>) -> RxResult<Arc<RxQueryBase>> {
        Err(plugin_missing_rx_error("query-builder"))
    }

    pub fn limit(&self, _amount: Option<u64>) -> RxResult<Arc<RxQueryBase>> {
        Err(plugin_missing_rx_error("query-builder"))
    }
}

#[derive(Debug, Clone)]
pub enum QueryExecutionResult {
    Documents(Vec<Value>),
    Count(u64),
    DocumentsById(HashMap<String, Value>),
}

#[derive(Clone)]
pub enum RxQueryExecResult {
    Documents(Vec<Arc<RxDocument>>),
    Document(Option<Arc<RxDocument>>),
    DocumentsById(HashMap<String, Arc<RxDocument>>),
    Count(u64),
}

// ref: rxdb/src/rx-query.ts _getDefaultQuery
pub fn get_default_query() -> MangoQuery {
    MangoQuery {
        selector: Some(json!({})),
        ..Default::default()
    }
}

// ref: rxdb/src/rx-query.ts createRxQuery
pub fn create_rx_query(
    op: RxQueryOp,
    query_obj: MangoQuery,
    collection: Arc<RxCollection>,
    other: Option<Value>,
) -> Arc<RxQueryBase> {
    let mut payload = json!({
        "op": op.as_str(),
        "queryObj": query_obj,
        "collection": collection.name,
        "other": other,
    });
    run_plugin_hooks("preCreateRxQuery", &mut payload);
    let query = Arc::new(RxQueryBase::new(
        op,
        Some(query_obj),
        Arc::clone(&collection),
        other,
    ));
    collection.get_by_query_cache(query)
}

// ref: rxdb/src/rx-query.ts queryCollection
pub async fn query_collection(rx_query: &RxQueryBase) -> RxResult<Vec<Value>> {
    if let Some(ids) = rx_query.is_find_one_by_id_query.as_ref() {
        let doc_cache = rx_query.collection.doc_cache()?;
        let primary_path = rx_query.collection.primary_path().ok_or_else(|| {
            new_rx_error(
                "QU_SCHEMA",
                Some(json!({ "collection": rx_query.collection.name })),
            )
        })?;
        let mut docs_by_id = HashMap::new();
        let mut missing_ids = Vec::new();
        for id in ids {
            if let Some(doc_data) = doc_cache.get_latest_document_data_if_exists(id) {
                if !doc_data
                    .get("_deleted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    docs_by_id.insert(id.clone(), doc_data);
                }
            } else {
                missing_ids.push(id.clone());
            }
        }
        if !missing_ids.is_empty() {
            let docs_from_storage = rx_query
                .collection
                .storage_instance
                .find_documents_by_id(&missing_ids, false)
                .await?;
            for doc in docs_from_storage {
                let deleted = doc
                    .get("_deleted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if deleted {
                    continue;
                }
                if let Some(id) = doc.get(&primary_path).and_then(Value::as_str) {
                    docs_by_id.insert(id.to_string(), doc);
                }
            }
        }
        let mut docs = Vec::new();
        for id in ids {
            if let Some(doc) = docs_by_id.remove(id) {
                docs.push(doc);
            }
        }
        if rx_query.op == RxQueryOp::FindOne && docs.len() > 1 {
            docs.truncate(1);
        }
        return Ok(docs);
    }

    let prepared = rx_query.get_prepared_query()?;
    let result = rx_query
        .collection
        .storage_instance
        .query(&prepared)
        .await?;
    let mut docs = result.documents;
    if rx_query.op == RxQueryOp::FindOne && docs.len() > 1 {
        docs.truncate(1);
    }
    Ok(docs)
}

// ref: rxdb/src/rx-query.ts isFindOneByIdQuery
pub fn is_find_one_by_id_query(primary_path: &str, query: &MangoQuery) -> Option<Vec<String>> {
    let selector = query.selector.as_ref()?.as_object()?;
    if selector.len() != 1 {
        return None;
    }
    let matcher = selector.get(primary_path)?;
    if let Some(id) = matcher.as_str() {
        return Some(vec![id.to_string()]);
    }
    let matcher_obj = matcher.as_object()?;
    if matcher_obj.len() != 1 {
        return None;
    }
    if let Some(eq) = matcher_obj.get("$eq").and_then(Value::as_str) {
        return Some(vec![eq.to_string()]);
    }
    matcher_obj.get("$in").and_then(Value::as_array).map(|ids| {
        ids.iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect()
    })
}

fn add_deleted_false_selector(query: &mut FilledMangoQuery) {
    if let Some(selector) = query.selector.as_object_mut() {
        selector.insert("_deleted".to_string(), json!({ "$eq": false }));
    }
}

fn find_by_ids_ids(query: &MangoQuery, primary_path: &str) -> RxResult<Vec<String>> {
    query
        .selector
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|selector| selector.get(primary_path))
        .and_then(Value::as_object)
        .and_then(|matcher| matcher.get("$in"))
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .ok_or_else(|| {
            new_rx_error(
                "QU_FIND_BY_IDS",
                Some(json!({ "primaryPath": primary_path, "query": query })),
            )
        })
}

fn collect_document_ids(value: &Value, op: RxQueryOp, primary_path: &str) -> Vec<String> {
    collect_documents(value, op)
        .into_iter()
        .filter_map(|document| {
            document
                .get(primary_path)
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect()
}

fn collect_documents(value: &Value, op: RxQueryOp) -> Vec<Value> {
    match value {
        Value::Null => Vec::new(),
        Value::Array(documents) => documents.clone(),
        Value::Object(map) if op == RxQueryOp::FindByIds => map.values().cloned().collect(),
        document => vec![document.clone()],
    }
}

fn project_documents_like_query_result(
    op: RxQueryOp,
    original: Value,
    primary_path: &str,
    documents_by_id: &HashMap<String, Value>,
) -> RxResult<Value> {
    match op {
        RxQueryOp::FindOne => {
            let Some(id) = original.get(primary_path).and_then(Value::as_str) else {
                return Ok(Value::Null);
            };
            Ok(documents_by_id.get(id).cloned().unwrap_or(Value::Null))
        }
        RxQueryOp::FindByIds => {
            let Some(map) = original.as_object() else {
                return Ok(Value::Array(Vec::new()));
            };
            Ok(Value::Array(
                map.keys()
                    .filter_map(|id| documents_by_id.get(id).cloned())
                    .collect(),
            ))
        }
        RxQueryOp::Find => {
            let Some(documents) = original.as_array() else {
                return Ok(Value::Array(Vec::new()));
            };
            Ok(Value::Array(
                documents
                    .iter()
                    .filter_map(|document| {
                        document
                            .get(primary_path)
                            .and_then(Value::as_str)
                            .and_then(|id| documents_by_id.get(id).cloned())
                    })
                    .collect(),
            ))
        }
        RxQueryOp::Count => Err(new_rx_error(
            "QU_MUTATE_COUNT",
            Some(json!({ "message": "count queries cannot modify documents" })),
        )),
    }
}

fn plugin_missing_rx_error(plugin_key: &str) -> crate::rx_error::RxError {
    let err = plugin_missing(plugin_key);
    new_rx_error(
        "PLUGIN_MISSING",
        Some(json!({
            "plugin": plugin_key,
            "message": err.to_string(),
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
    use crate::rx_collection::RxCollection;
    use crate::rx_database::RxDatabase;
    use crate::rx_error::RxError;
    use crate::rx_schema::create_rx_schema;
    use crate::rxjs_compat::RxStream;
    use crate::types::{
        BulkWriteRow, EventBulk, HashFunction, HashOutput, JsonSchema, PrimaryKey, RxJsonSchema,
        RxStorageBulkWriteResponse, RxStorageChangedDocumentsSinceResult, RxStorageCountResult,
        RxStorageInstance, RxStorageInstanceCreationParams, RxStorageQueryResult,
    };
    use async_trait::async_trait;

    struct TestHashFunction;

    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    struct CountingStorageInstance {
        inner: Arc<dyn RxStorageInstance>,
        query_count: AtomicUsize,
        find_by_id_count: AtomicUsize,
        slow_count_mode: AtomicBool,
    }

    impl CountingStorageInstance {
        fn new(inner: Arc<dyn RxStorageInstance>) -> Self {
            Self {
                inner,
                query_count: AtomicUsize::new(0),
                find_by_id_count: AtomicUsize::new(0),
                slow_count_mode: AtomicBool::new(false),
            }
        }

        fn set_slow_count_mode(&self, slow: bool) {
            self.slow_count_mode.store(slow, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl RxStorageInstance for CountingStorageInstance {
        fn database_name(&self) -> &str {
            self.inner.database_name()
        }

        fn collection_name(&self) -> &str {
            self.inner.collection_name()
        }

        fn schema(&self) -> &RxJsonSchema {
            self.inner.schema()
        }

        async fn bulk_write(
            &self,
            document_writes: Vec<BulkWriteRow>,
            context: &str,
        ) -> Result<RxStorageBulkWriteResponse, RxError> {
            self.inner.bulk_write(document_writes, context).await
        }

        async fn find_documents_by_id(
            &self,
            ids: &[String],
            with_deleted: bool,
        ) -> Result<Vec<Value>, RxError> {
            self.find_by_id_count.fetch_add(1, Ordering::SeqCst);
            self.inner.find_documents_by_id(ids, with_deleted).await
        }

        async fn query(&self, prepared_query: &Value) -> Result<RxStorageQueryResult, RxError> {
            self.query_count.fetch_add(1, Ordering::SeqCst);
            self.inner.query(prepared_query).await
        }

        async fn count(&self, prepared_query: &Value) -> Result<RxStorageCountResult, RxError> {
            let mut result = self.inner.count(prepared_query).await?;
            if self.slow_count_mode.load(Ordering::SeqCst) {
                result.mode = "slow".to_string();
            }
            Ok(result)
        }

        async fn get_changed_documents_since(
            &self,
            limit: u64,
            checkpoint: Option<&Value>,
        ) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
            self.inner
                .get_changed_documents_since(limit, checkpoint)
                .await
        }

        fn change_stream(&self) -> RxStream<EventBulk> {
            self.inner.change_stream()
        }

        async fn cleanup(&self, min_deleted_time: i64) -> Result<bool, RxError> {
            self.inner.cleanup(min_deleted_time).await
        }

        async fn remove(&self) -> Result<(), RxError> {
            self.inner.remove().await
        }

        async fn close(&self) -> Result<(), RxError> {
            self.inner.close().await
        }

        async fn get_attachment_data(
            &self,
            document_id: &str,
            attachment_id: &str,
            digest: &str,
        ) -> Result<String, RxError> {
            self.inner
                .get_attachment_data(document_id, attachment_id, digest)
                .await
        }

        fn underlying_persistent_storage(&self) -> Option<Arc<dyn RxStorageInstance>> {
            self.inner.underlying_persistent_storage()
        }
    }

    fn raw_schema() -> RxJsonSchema {
        let mut properties = HashMap::new();
        properties.insert(
            "id".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                max_length: Some(100),
                ..Default::default()
            },
        );
        properties.insert(
            "age".to_string(),
            JsonSchema {
                schema_type: Some("number".to_string()),
                ..Default::default()
            },
        );
        RxJsonSchema {
            version: 0,
            primary_key: PrimaryKey::Simple("id".to_string()),
            schema_type: "object".to_string(),
            properties,
            required: vec!["id".to_string()],
            indexes: vec![vec!["age".to_string()]],
            encrypted: Vec::new(),
            internal_indexes: Vec::new(),
            key_compression: false,
            attachments: None,
            additional_properties: false,
            extra: HashMap::new(),
        }
    }

    fn doc(id: &str, age: i64, deleted: bool, lwt: f64) -> Value {
        json!({
            "id": id,
            "age": age,
            "_rev": "1-test",
            "_deleted": deleted,
            "_meta": { "lwt": lwt },
            "_attachments": {}
        })
    }

    async fn test_collection() -> Arc<RxCollection> {
        let hash_function = Arc::new(TestHashFunction);
        let schema =
            Arc::new(create_rx_schema(raw_schema(), hash_function.clone(), false).unwrap());
        let storage = get_rx_storage_memory(());
        let storage_instance = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.json_schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        storage_instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: None,
                        document: doc("a", 1, false, 1.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("b", 3, false, 2.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("c", 2, true, 3.0),
                    },
                ],
                "seed",
            )
            .await
            .unwrap();
        let database = RxDatabase::new(
            "db",
            "db-token",
            "storage-token",
            false,
            hash_function,
            storage,
        );
        RxCollection::new_with_schema(
            "docs",
            database,
            storage_instance,
            Arc::new(DefaultConflictHandler),
            schema,
        )
    }

    #[tokio::test]
    async fn exec_find_filters_deleted_and_sorts() {
        let collection = test_collection().await;
        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "desc".to_string());
        let query = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({})),
                sort: Some(vec![sort]),
                index: None,
                limit: None,
                skip: None,
            },
            collection,
            None,
        );

        let value = query.exec(false).await.unwrap();
        let docs = value.as_array().unwrap();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].get("id").and_then(Value::as_str), Some("b"));
        assert_eq!(docs[1].get("id").and_then(Value::as_str), Some("a"));
    }

    #[tokio::test]
    async fn exec_find_one_and_throw_if_missing() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::FindOne,
            MangoQuery {
                selector: Some(json!({ "id": { "$eq": "a" } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        assert_eq!(
            query
                .exec(true)
                .await
                .unwrap()
                .get("id")
                .and_then(Value::as_str),
            Some("a")
        );

        let missing = create_rx_query(
            RxQueryOp::FindOne,
            MangoQuery {
                selector: Some(json!({ "id": { "$eq": "missing" } })),
                ..Default::default()
            },
            collection,
            None,
        );
        assert!(missing.exec(true).await.is_err());
        assert_eq!(missing.exec(false).await.unwrap(), Value::Null);
    }

    #[tokio::test]
    async fn exec_rx_documents_materializes_cached_document_handles() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::FindOne,
            MangoQuery {
                selector: Some(json!({ "id": { "$eq": "a" } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        let first = match query.exec_rx_documents(true).await.unwrap() {
            RxQueryExecResult::Document(Some(document)) => document,
            _ => panic!("findOne should return one RxDocument handle"),
        };
        assert_eq!(first.primary().unwrap(), "a");

        let second = match query.exec_rx_documents(true).await.unwrap() {
            RxQueryExecResult::Document(Some(document)) => document,
            _ => panic!("findOne should return one RxDocument handle"),
        };
        assert!(Arc::ptr_eq(&first, &second));

        let find = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            collection,
            None,
        );
        let docs = match find.exec_rx_documents(false).await.unwrap() {
            RxQueryExecResult::Documents(documents) => documents,
            _ => panic!("find should return RxDocument handles"),
        };
        assert_eq!(docs.len(), 2);
        assert!(docs.iter().all(|document| !document.deleted()));
    }

    #[tokio::test]
    async fn result_stream_emits_initial_and_changed_query_results() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        let mut results = query.result_stream().unwrap();

        let initial = match results.next().await.unwrap() {
            RxQueryExecResult::Documents(documents) => documents,
            _ => panic!("find query should emit document arrays"),
        };
        assert_eq!(initial.len(), 2);

        let collection_for_write = Arc::clone(&collection);
        let writer = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            collection_for_write
                .storage_instance
                .bulk_write(
                    vec![BulkWriteRow {
                        previous: None,
                        document: doc("d", 4, false, 4.0),
                    }],
                    "test-query-result-stream",
                )
                .await
                .unwrap();
        });

        let changed = tokio::time::timeout(std::time::Duration::from_secs(1), results.next())
            .await
            .unwrap();
        writer.await.unwrap();
        let changed = match changed.unwrap() {
            RxQueryExecResult::Documents(documents) => documents,
            _ => panic!("find query should emit document arrays"),
        };
        assert_eq!(changed.len(), 3);
        assert!(changed
            .iter()
            .any(|document| document.primary().unwrap() == "d"));
    }

    #[tokio::test]
    async fn query_double_dollar_exposes_behavior_subject_result() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        let signal = query.double_dollar().unwrap();
        let mut values = signal.subscribe();
        assert!(values.next().await.unwrap().is_none());

        let first = tokio::time::timeout(std::time::Duration::from_secs(1), values.next())
            .await
            .unwrap()
            .unwrap();
        let Some(RxQueryExecResult::Documents(documents)) = first else {
            panic!("find query should emit document arrays");
        };
        assert_eq!(documents.len(), 2);

        collection
            .storage_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("d", 4, false, 4.0),
                }],
                "test-query-double-dollar",
            )
            .await
            .unwrap();
        let changed = tokio::time::timeout(std::time::Duration::from_secs(1), values.next())
            .await
            .unwrap()
            .unwrap();
        let Some(RxQueryExecResult::Documents(documents)) = changed else {
            panic!("find query should emit changed document arrays");
        };
        assert!(documents.len() >= 3);
    }

    #[tokio::test]
    async fn query_collection_uses_find_by_id_shortcut_for_primary_key_queries() {
        let base_collection = test_collection().await;
        let spy = Arc::new(CountingStorageInstance::new(Arc::clone(
            &base_collection.storage_instance,
        )));
        let collection = RxCollection::new_with_schema(
            "docs",
            Arc::clone(&base_collection.database),
            Arc::clone(&spy) as Arc<dyn RxStorageInstance>,
            Arc::new(DefaultConflictHandler),
            Arc::clone(base_collection.schema.as_ref().unwrap()),
        );

        let find_one = create_rx_query(
            RxQueryOp::FindOne,
            MangoQuery {
                selector: Some(json!({ "id": { "$eq": "a" } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        let found = find_one.exec(false).await.unwrap();
        assert_eq!(found.get("id").and_then(Value::as_str), Some("a"));
        assert_eq!(spy.query_count.load(Ordering::SeqCst), 0);
        assert_eq!(spy.find_by_id_count.load(Ordering::SeqCst), 1);

        let find_many = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({ "id": { "$in": ["b", "a", "c"] } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        let result = find_many.exec(false).await.unwrap();
        let ids: Vec<_> = result
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|doc| doc.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["b", "a"]);
        assert_eq!(spy.query_count.load(Ordering::SeqCst), 0);
        assert_eq!(spy.find_by_id_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn exec_count_and_find_by_ids() {
        let collection = test_collection().await;
        let count = create_rx_query(
            RxQueryOp::Count,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        assert_eq!(count.exec(false).await.unwrap(), json!(2));

        let by_ids = create_rx_query(
            RxQueryOp::FindByIds,
            MangoQuery {
                selector: Some(json!({ "id": { "$in": ["a", "c", "missing"] } })),
                ..Default::default()
            },
            collection,
            None,
        );
        let value = by_ids.exec(false).await.unwrap();
        let map = value.as_object().unwrap();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("a"));
    }

    #[tokio::test]
    async fn run_query_update_function_preserves_query_result_shape() {
        let collection = test_collection().await;
        let find = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        let updated = crate::rx_query_helper::run_query_update_function(&find, |mut doc| {
            Box::pin(async move {
                doc.as_object_mut()
                    .unwrap()
                    .insert("updated".to_string(), json!(true));
                Ok(doc)
            })
        })
        .await
        .unwrap();
        let updated_docs = updated.as_array().unwrap();
        assert_eq!(updated_docs.len(), 2);
        assert!(updated_docs
            .iter()
            .all(|doc| doc.get("updated") == Some(&json!(true))));

        let find_one_missing = create_rx_query(
            RxQueryOp::FindOne,
            MangoQuery {
                selector: Some(json!({ "id": { "$eq": "missing" } })),
                ..Default::default()
            },
            collection,
            None,
        );
        let updated_missing =
            crate::rx_query_helper::run_query_update_function(&find_one_missing, |doc| {
                Box::pin(async move { Ok(doc) })
            })
            .await
            .unwrap();
        assert_eq!(updated_missing, Value::Null);
    }

    #[tokio::test]
    async fn query_patch_and_remove_persist_documents() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 1 } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        let patched = query.patch(json!({ "age": 9 })).await.unwrap();
        let patched_docs = patched.as_array().unwrap();
        assert_eq!(patched_docs.len(), 2);
        assert!(patched_docs
            .iter()
            .all(|doc| doc.get("age") == Some(&json!(9))));

        let count = create_rx_query(
            RxQueryOp::Count,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        assert_eq!(count.exec(false).await.unwrap(), json!(2));

        let removed = query.remove().await.unwrap();
        let removed_docs = removed.as_array().unwrap();
        assert_eq!(removed_docs.len(), 2);
        assert!(removed_docs
            .iter()
            .all(|doc| doc.get("_deleted") == Some(&json!(true))));
        assert_eq!(count.exec(false).await.unwrap(), json!(0));
    }

    #[tokio::test]
    async fn incremental_remove_persists_like_remove() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::FindOne,
            MangoQuery {
                selector: Some(json!({ "id": { "$eq": "a" } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        let removed = query.incremental_remove().await.unwrap();
        assert_eq!(removed.get("_deleted"), Some(&json!(true)));
        assert_eq!(removed.get("id"), Some(&json!("a")));

        assert_eq!(query.exec(false).await.unwrap(), Value::Null);
    }

    #[tokio::test]
    async fn plugin_backed_query_methods_return_plugin_missing() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        assert_eq!(
            query.update(json!({})).unwrap_err().code(),
            "PLUGIN_MISSING"
        );
        for result in [
            query.where_(json!({})),
            query.sort(json!("age")),
            query.skip(Some(1)),
            query.limit(Some(1)),
        ] {
            match result {
                Ok(_) => panic!("plugin-backed query method must fail without plugin"),
                Err(err) => assert_eq!(err.code(), "PLUGIN_MISSING"),
            }
        }
    }

    #[tokio::test]
    async fn create_rx_query_reuses_cached_query_instance() {
        let collection = test_collection().await;
        let query_a = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 1 } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        let query_b = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 1 } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );
        let query_c = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 4 } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        assert!(Arc::ptr_eq(&query_a, &query_b));
        assert!(!Arc::ptr_eq(&query_a, &query_c));
        assert_eq!(collection.query_cache_size(), 2);
    }

    #[tokio::test]
    async fn collection_query_cache_replacement_uncaches_old_unexecuted_queries() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 1 } })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        collection.run_query_cache_replacement(0, -1.0);

        assert!(query.is_uncached());
        assert_eq!(collection.query_cache_size(), 0);
    }

    #[tokio::test]
    async fn ensure_equal_skips_storage_when_change_buffer_is_in_sync() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        query.exec(false).await.unwrap();
        assert_eq!(*query.exec_over_database_count.lock(), 1);

        query.exec(false).await.unwrap();
        assert_eq!(*query.exec_over_database_count.lock(), 1);

        collection
            .storage_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("d", 4, false, 4.0),
                }],
                "test-change-buffer",
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let result = query.exec(false).await.unwrap();
        assert_eq!(
            *query.exec_over_database_count.lock(),
            1,
            "unpaginated find insert should be patched without a storage re-query"
        );
        assert!(result
            .as_array()
            .unwrap()
            .iter()
            .any(|doc| doc.get("id") == Some(&json!("d"))));
    }

    #[tokio::test]
    async fn count_query_updates_from_change_buffer_without_storage_reexec() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::Count,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        assert_eq!(query.exec(false).await.unwrap(), json!(2));
        assert_eq!(*query.exec_over_database_count.lock(), 1);

        collection
            .storage_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("d", 4, false, 4.0),
                }],
                "test-count-event-reduce",
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(query.exec(false).await.unwrap(), json!(3));
        assert_eq!(
            *query.exec_over_database_count.lock(),
            1,
            "count event-reduce should avoid a storage count re-execution"
        );
    }

    #[tokio::test]
    async fn find_query_ignores_irrelevant_change_events_without_storage_reexec() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::FindOne,
            MangoQuery {
                selector: Some(json!({ "id": "a" })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        assert_eq!(
            query.exec(false).await.unwrap().get("id"),
            Some(&json!("a"))
        );
        assert_eq!(*query.exec_over_database_count.lock(), 1);

        collection
            .storage_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("d", 1, false, 4.0),
                }],
                "test-find-event-reduce-noop",
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let result = query.exec(false).await.unwrap();
        assert_eq!(result.get("id"), Some(&json!("a")));
        assert_eq!(
            *query.exec_over_database_count.lock(),
            1,
            "irrelevant find change should not re-execute storage query"
        );
    }

    #[tokio::test]
    async fn find_query_patches_matching_changes_without_storage_reexec() {
        let collection = test_collection().await;
        let query = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        let initial = query.exec(false).await.unwrap();
        let initial_docs = initial.as_array().unwrap();
        assert_eq!(initial_docs.len(), 2);
        assert_eq!(initial_docs[0].get("id"), Some(&json!("a")));
        assert_eq!(initial_docs[1].get("id"), Some(&json!("b")));
        assert_eq!(*query.exec_over_database_count.lock(), 1);

        collection
            .storage_instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: None,
                        document: doc("d", 3, false, 4.0),
                    },
                    BulkWriteRow {
                        previous: Some(doc("a", 1, false, 1.0)),
                        document: doc("a", 4, false, 5.0),
                    },
                    BulkWriteRow {
                        previous: Some(doc("b", 3, false, 2.0)),
                        document: doc("b", 3, true, 6.0),
                    },
                ],
                "test-find-event-reduce-patch",
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let result = query.exec(false).await.unwrap();
        let docs = result.as_array().unwrap();
        let ids: Vec<_> = docs
            .iter()
            .filter_map(|doc| doc.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["d", "a"]);
        assert_eq!(
            *query.exec_over_database_count.lock(),
            1,
            "unpaginated find event-reduce should patch cached results"
        );
    }

    #[tokio::test]
    async fn disabled_event_reduce_forces_find_query_reexecution() {
        let base_collection = test_collection().await;
        let database = RxDatabase::new_with_query_options(
            "db",
            "db-token-no-event-reduce",
            "storage-token",
            false,
            Arc::new(TestHashFunction),
            Arc::clone(&base_collection.database.storage),
            false,
            false,
        );
        let collection = RxCollection::new_with_schema(
            "docs",
            database,
            Arc::clone(&base_collection.storage_instance),
            Arc::new(DefaultConflictHandler),
            Arc::clone(base_collection.schema.as_ref().unwrap()),
        );
        let query = create_rx_query(
            RxQueryOp::FindOne,
            MangoQuery {
                selector: Some(json!({ "id": "a" })),
                ..Default::default()
            },
            Arc::clone(&collection),
            None,
        );

        assert_eq!(
            query.exec(false).await.unwrap().get("id"),
            Some(&json!("a"))
        );
        assert_eq!(*query.exec_over_database_count.lock(), 1);
        collection
            .storage_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("d", 1, false, 4.0),
                }],
                "test-disabled-event-reduce",
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(
            query.exec(false).await.unwrap().get("id"),
            Some(&json!("a"))
        );
        assert_eq!(*query.exec_over_database_count.lock(), 2);
    }

    #[tokio::test]
    async fn slow_count_requires_database_allow_slow_count() {
        let base_collection = test_collection().await;
        let spy = Arc::new(CountingStorageInstance::new(Arc::clone(
            &base_collection.storage_instance,
        )));
        spy.set_slow_count_mode(true);
        let database = RxDatabase::new_with_query_options(
            "db",
            "db-token-slow-count",
            "storage-token",
            false,
            Arc::new(TestHashFunction),
            Arc::clone(&base_collection.database.storage),
            true,
            false,
        );
        let collection = RxCollection::new_with_schema(
            "docs",
            database,
            Arc::clone(&spy) as Arc<dyn RxStorageInstance>,
            Arc::new(DefaultConflictHandler),
            Arc::clone(base_collection.schema.as_ref().unwrap()),
        );
        let query = create_rx_query(
            RxQueryOp::Count,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            collection,
            None,
        );

        let Err(err) = query.exec(false).await else {
            panic!("slow count must be rejected without allow_slow_count");
        };
        assert_eq!(err.code(), "QU14");

        let database = RxDatabase::new_with_query_options(
            "db",
            "db-token-slow-count-allowed",
            "storage-token",
            false,
            Arc::new(TestHashFunction),
            Arc::clone(&base_collection.database.storage),
            true,
            true,
        );
        let collection = RxCollection::new_with_schema(
            "docs",
            database,
            Arc::clone(&spy) as Arc<dyn RxStorageInstance>,
            Arc::new(DefaultConflictHandler),
            Arc::clone(base_collection.schema.as_ref().unwrap()),
        );
        let query = create_rx_query(
            RxQueryOp::Count,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            collection,
            None,
        );
        assert_eq!(query.exec(false).await.unwrap(), json!(2));
    }

    #[tokio::test]
    async fn ensure_equal_awaits_collection_before_read_callbacks() {
        let collection = test_collection().await;
        let calls = Arc::new(AtomicUsize::new(0));
        collection.add_await_before_read({
            let calls = Arc::clone(&calls);
            Arc::new(move || {
                let calls = Arc::clone(&calls);
                Box::pin(async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            })
        });
        let query = create_rx_query(
            RxQueryOp::Find,
            MangoQuery {
                selector: Some(json!({})),
                ..Default::default()
            },
            collection,
            None,
        );

        query.exec(false).await.unwrap();
        query.exec(false).await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(*query.exec_over_database_count.lock(), 1);
    }

    #[test]
    fn detects_find_one_by_id_query_shapes() {
        assert_eq!(
            is_find_one_by_id_query(
                "id",
                &MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "a" } })),
                    ..Default::default()
                }
            ),
            Some(vec!["a".to_string()])
        );
        assert_eq!(
            is_find_one_by_id_query(
                "id",
                &MangoQuery {
                    selector: Some(json!({ "id": { "$in": ["a", "b"] } })),
                    ..Default::default()
                }
            ),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }
}
