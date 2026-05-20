//! Port of `src/rx-query-helper.ts`.
//!
//! The storage-relevant query helpers and the update-function runner are ported:
//! - `normalize_mango_query`
//! - `get_sort_comparator`
//! - `get_query_matcher`
//! - `run_query_update_function`
//! - `prepare_query`
//!
//! `run_query_update_function` operates on the current Rust query result shape
//! (`serde_json::Value`) instead of JS `RxDocument` instances. Once `RxQuery`
//! materializes `RxDocument`s for all result modes, this function can narrow to
//! document handles without changing the control flow.
//!
//! T1 deviations:
//! - `mingoSortComparator` (`mingo/util.compare`) is inlined as
//!   [`value_compare`] — handles the JSON value types that can appear in
//!   indexed documents (numbers, strings, bools, null). Arrays and objects
//!   compare by `serde_json::to_string` representation for determinism.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::plugins::utils::utils_object::{clone_deep, flat_clone, object_path_monad};
use crate::query_planner::{get_query_plan, LOGICAL_OPERATORS};
use crate::rx_error::{new_rx_error, RxResult};
use crate::rx_schema_helper::get_primary_field_of_primary_key;
use crate::types::{FilledMangoQuery, MangoQuery, PreparedQuery, RxJsonSchema};
use crate::util::mango::get_mingo_query;

/// Closure type for sort comparators (`DeterministicSortComparator` upstream).
pub type DeterministicSortComparator = Arc<dyn Fn(&Value, &Value) -> Ordering + Send + Sync>;

/// Closure type for query matchers (`QueryMatcher` upstream).
pub type QueryMatcher = Arc<dyn Fn(&Value) -> bool + Send + Sync>;

// ref: rxdb/src/rx-query-helper.ts:35-176
/// Normalize the query to ensure all fields are set and equivalent logical
/// queries compare equal by cache key.
pub fn normalize_mango_query(schema: &RxJsonSchema, mango_query: MangoQuery) -> FilledMangoQuery {
    let primary_key = get_primary_field_of_primary_key(&schema.primary_key);

    let mut selector = mango_query
        .selector
        .clone()
        .unwrap_or_else(|| Value::Object(Default::default()));
    // Normalize bare-value equality into `$eq` form.
    if let Some(obj) = selector.as_object_mut() {
        let keys: Vec<String> = obj.keys().cloned().collect();
        for field in keys {
            let entry = obj.get(&field).cloned().unwrap_or(Value::Null);
            if !entry.is_object() {
                obj.insert(field, json!({ "$eq": entry }));
            }
        }
    }

    let mut index_opt = mango_query.index.clone();
    if let Some(idx) = &mut index_opt {
        if !idx.contains(&primary_key) {
            idx.push(primary_key.clone());
        }
    }

    let skip = mango_query.skip.unwrap_or(0);
    let limit = mango_query.limit;

    // Determine sort.
    let mut sort: Vec<HashMap<String, String>> = match mango_query.sort {
        Some(s) => s,
        None => {
            // If an index was specified, mirror it for sort order.
            if let Some(idx) = &index_opt {
                idx.iter()
                    .map(|f| {
                        let mut m = HashMap::new();
                        m.insert(f.clone(), "asc".to_string());
                        m
                    })
                    .collect()
            } else {
                // Find an index that best matches the fields-with-logical-operator.
                let mut fields_with_logical: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                if let Some(sel_obj) = selector.as_object() {
                    for (field, matcher) in sel_obj.iter() {
                        let has_logical = match matcher.as_object() {
                            Some(m) => m.keys().any(|k| LOGICAL_OPERATORS.contains(k.as_str())),
                            None => true,
                        };
                        if has_logical {
                            fields_with_logical.insert(field.clone());
                        }
                    }
                }
                let mut best_amount: i64 = -1;
                let mut best_index: Option<&Vec<String>> = None;
                for index in schema.indexes.iter() {
                    let first_wrong = index
                        .iter()
                        .position(|f| !fields_with_logical.contains(f))
                        .map(|p| p as i64)
                        .unwrap_or(index.len() as i64);
                    if first_wrong > 0 && first_wrong > best_amount {
                        best_amount = first_wrong;
                        best_index = Some(index);
                    }
                }
                if let Some(idx) = best_index {
                    idx.iter()
                        .map(|f| {
                            let mut m = HashMap::new();
                            m.insert(f.clone(), "asc".to_string());
                            m
                        })
                        .collect()
                } else if !schema.indexes.is_empty() {
                    // Use first index of schema.
                    schema.indexes[0]
                        .iter()
                        .map(|f| {
                            let mut m = HashMap::new();
                            m.insert(f.clone(), "asc".to_string());
                            m
                        })
                        .collect()
                } else {
                    // Fall back to primary key.
                    let mut m = HashMap::new();
                    m.insert(primary_key.clone(), "asc".to_string());
                    vec![m]
                }
            }
        }
    };

    // Ensure primary key is in sort.
    let has_primary = sort
        .iter()
        .any(|m| m.keys().next().map(|k| k == &primary_key).unwrap_or(false));
    if !has_primary {
        let mut m = HashMap::new();
        m.insert(primary_key.clone(), "asc".to_string());
        sort.push(m);
    }

    FilledMangoQuery {
        selector,
        sort,
        index: index_opt,
        limit,
        skip: Some(skip),
    }
}

// ref: rxdb/src/rx-query-helper.ts:183-217
/// Returns a sort comparator that orders documents the same way a query
/// would order results.
pub fn get_sort_comparator(
    _schema: &RxJsonSchema,
    query: &FilledMangoQuery,
) -> DeterministicSortComparator {
    let mut parts: Vec<(String, String, Box<dyn Fn(&Value) -> Value + Send + Sync>)> = Vec::new();
    for sort_block in query.sort.iter() {
        if let Some((k, dir)) = sort_block.iter().next() {
            let getter = object_path_monad(k);
            parts.push((k.clone(), dir.clone(), getter));
        }
    }
    Arc::new(move |a: &Value, b: &Value| -> Ordering {
        for (_k, dir, getter) in parts.iter() {
            let va = getter(a);
            let vb = getter(b);
            if va != vb {
                return if dir == "asc" {
                    value_compare(&va, &vb)
                } else {
                    value_compare(&vb, &va)
                };
            }
        }
        Ordering::Equal
    })
}

// ref: rxdb/src/rx-query-helper.ts:225-238
/// Returns a function that checks if a document matches the query.
pub fn get_query_matcher(_schema: &RxJsonSchema, query: &FilledMangoQuery) -> QueryMatcher {
    let mingo_query = get_mingo_query(&query.selector);
    Arc::new(move |doc: &Value| -> bool { mingo_query.test(doc) })
}

// ref: rxdb/src/rx-query-helper.ts:241-266
/// Execute a query and apply an async update function to each returned
/// document. `findOne` preserves its single/null result shape; array and
/// `findByIds` map results return an array of updated documents like upstream
/// `Promise.all(...)`.
pub async fn run_query_update_function<F, Fut>(
    rx_query: &crate::rx_query::RxQueryBase,
    mut update_fn: F,
) -> RxResult<Value>
where
    F: FnMut(Value) -> Fut,
    Fut: Future<Output = RxResult<Value>>,
{
    let docs = rx_query.exec(false).await?;
    match docs {
        Value::Null => Ok(Value::Null),
        Value::Array(documents) => {
            let mut updated = Vec::with_capacity(documents.len());
            for document in documents {
                updated.push(update_fn(document).await?);
            }
            Ok(Value::Array(updated))
        }
        Value::Object(map) if rx_query.op == crate::rx_query::RxQueryOp::FindByIds => {
            let mut updated = Vec::with_capacity(map.len());
            for document in map.into_values() {
                updated.push(update_fn(document).await?);
            }
            Ok(Value::Array(updated))
        }
        document => update_fn(document).await,
    }
}

// ref: rxdb/src/rx-query-helper.ts:269-292
/// Returns a format of the query that can be used with the storage
/// when calling `RxStorageInstance::query()`.
pub fn prepare_query(
    schema: &RxJsonSchema,
    mutable_query: FilledMangoQuery,
) -> RxResult<PreparedQuery> {
    if mutable_query.sort.is_empty() {
        return Err(new_rx_error(
            "SNH",
            Some(json!({ "message": "prepare_query: query.sort is empty" })),
        ));
    }
    let query_plan = get_query_plan(schema, &mutable_query)?;
    // PreparedQuery is a `serde_json::Value` (type alias) with shape
    // `{ query: <mutable_query>, queryPlan: <query_plan> }`.
    Ok(json!({
        "query": serde_json::to_value(&mutable_query).unwrap_or(Value::Null),
        "queryPlan": serde_json::to_value(&query_plan).unwrap_or(Value::Null),
    }))
}

/// Inlined `mingo/util.compare` analogue. Handles JSON-shape values.
fn value_compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(fx), Some(fy)) => fx.partial_cmp(&fy).unwrap_or(Ordering::Equal),
            _ => Ordering::Equal,
        },
        (Value::String(x), Value::String(y)) => x.cmp(y),
        // Arrays and objects compare by their canonical JSON serialization —
        // not semantically meaningful for arbitrary structures, but
        // deterministic which is what callers need.
        _ => {
            let sa = serde_json::to_string(a).unwrap_or_default();
            let sb = serde_json::to_string(b).unwrap_or_default();
            sa.cmp(&sb)
        }
    }
}

/// Keep `clone_deep` / `flat_clone` available as imports for future
/// document-handle parity work.
#[allow(dead_code)]
fn _phantom_use(d: &Value) -> Value {
    let _ = flat_clone(d);
    clone_deep(d)
}
