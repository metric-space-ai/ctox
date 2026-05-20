//! Port of `src/query-planner.ts`.
//!
//! Returns the query plan which contains information about how to run the query
//! and which indexes to use. Used by storage backends (Memory, dexie, IndexedDB)
//! and by our own SQLite storage (gap-item N1).

use std::collections::HashSet;
use std::sync::LazyLock;

use serde_json::{json, Value};

use crate::plugins::utils::utils_array::count_until_not_matching;
use crate::rx_error::{new_rx_error, RxError};
use crate::rx_schema_helper::get_schema_by_object_path;
use crate::types::{
    FilledMangoQuery, MangoQuerySelector, PartialRxQueryPlanerOpts, RxJsonSchema, RxQueryPlan,
    RxQueryPlanKey, RxQueryPlanerOpts,
};

// ref: rxdb/src/query-planner.ts:14
/// Sentinel for the maximum-possible index key (upstream `String.fromCharCode(65535)`).
pub static INDEX_MAX: LazyLock<Value> = LazyLock::new(|| Value::String("\u{ffff}".to_string()));

// ref: rxdb/src/query-planner.ts:16-25
/// Sentinel for the minimum-possible index key. We avoid `-Infinity` because
/// it would be transformed to `null` on JSON.stringify(), which can break things
/// when the query plan is sent to the storage as JSON.
/// Value: `Number.MIN_SAFE_INTEGER` = -(2^53 - 1).
pub static INDEX_MIN: LazyLock<Value> = LazyLock::new(|| json!(-9_007_199_254_740_991_i64));

// ref: rxdb/src/query-planner.ts:175-177
pub static LOGICAL_OPERATORS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    ["$eq", "$gt", "$gte", "$lt", "$lte"]
        .iter()
        .copied()
        .collect()
});
pub static LOWER_BOUND_LOGICAL_OPERATORS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| ["$eq", "$gt", "$gte"].iter().copied().collect());
pub static UPPER_BOUND_LOGICAL_OPERATORS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| ["$eq", "$lt", "$lte"].iter().copied().collect());

// ref: rxdb/src/query-planner.ts:34-173
/// Returns the query plan which contains information about how to run the query
/// and which indexes to use.
pub fn get_query_plan(
    schema: &RxJsonSchema,
    query: &FilledMangoQuery,
) -> Result<RxQueryPlan, RxError> {
    let selector = &query.selector;
    let indexes: Vec<Vec<String>> = if let Some(idx) = &query.index {
        vec![idx.clone()]
    } else {
        schema.indexes.clone()
    };

    // Most storages do not support descending indexes
    // so having a 'desc' in the sorting, means we always have to re-sort the results.
    let has_desc_sorting = query
        .sort
        .iter()
        .any(|m| m.values().next().map(String::as_str) == Some("desc"));

    // Some fields can be part of the selector while not being relevant for sorting
    // because their selector operators specify that in all cases all matching docs
    // would have the same value (e.g. boolean field _deleted).
    let mut sort_irrelevant_fields: HashSet<String> = HashSet::new();
    if let Some(sel_obj) = selector.as_object() {
        for (field_name, value) in sel_obj.iter() {
            let schema_part = get_schema_by_object_path(schema, field_name);
            let is_boolean = schema_part.schema_type.as_deref() == Some("boolean");
            let has_eq = value.get("$eq").is_some();
            if is_boolean && has_eq {
                sort_irrelevant_fields.insert(field_name.clone());
            }
        }
    }

    let optimal_sort_index: Vec<String> = query
        .sort
        .iter()
        .map(|m| m.keys().next().cloned().unwrap_or_default())
        .collect();
    let optimal_sort_index_compare_string = optimal_sort_index
        .iter()
        .filter(|f| !sort_irrelevant_fields.contains(*f))
        .cloned()
        .collect::<Vec<_>>()
        .join(",");

    let mut current_best_quality: f64 = -1.0;
    let mut current_best_query_plan: Option<RxQueryPlan> = None;

    // Calculate one query plan for each index and then test which is best.
    for index in indexes.iter() {
        let mut inclusive_end = true;
        let mut inclusive_start = true;
        let mut opts_list: Vec<RxQueryPlanerOpts> = Vec::with_capacity(index.len());

        for index_field in index.iter() {
            let matcher = selector.get(index_field);
            let operators: Vec<&String> = matcher
                .and_then(|m| m.as_object())
                .map(|o| o.keys().collect())
                .unwrap_or_default();

            let mut partial = PartialRxQueryPlanerOpts::default();

            if matcher.is_none() || operators.is_empty() {
                let start_key = if inclusive_start {
                    INDEX_MIN.clone()
                } else {
                    INDEX_MAX.clone()
                };
                let end_key = if inclusive_end {
                    INDEX_MAX.clone()
                } else {
                    INDEX_MIN.clone()
                };
                partial = PartialRxQueryPlanerOpts {
                    start_key: Some(start_key),
                    end_key: Some(end_key),
                    inclusive_start: Some(true),
                    inclusive_end: Some(true),
                };
            } else {
                let matcher_obj = matcher.and_then(|m| m.as_object()).unwrap();
                for operator in operators.iter() {
                    if LOGICAL_OPERATORS.contains(operator.as_str()) {
                        let value = matcher_obj.get(*operator).cloned().unwrap_or(Value::Null);
                        let part = get_matcher_query_opts(operator, &value)?;
                        if part.start_key.is_some() {
                            partial.start_key = part.start_key;
                        }
                        if part.end_key.is_some() {
                            partial.end_key = part.end_key;
                        }
                        if part.inclusive_start.is_some() {
                            partial.inclusive_start = part.inclusive_start;
                        }
                        if part.inclusive_end.is_some() {
                            partial.inclusive_end = part.inclusive_end;
                        }
                    }
                }
            }

            // fill missing attributes
            let merged = RxQueryPlanerOpts {
                start_key: partial.start_key.unwrap_or_else(|| INDEX_MIN.clone()),
                end_key: partial.end_key.unwrap_or_else(|| INDEX_MAX.clone()),
                inclusive_start: partial.inclusive_start.unwrap_or(true),
                inclusive_end: partial.inclusive_end.unwrap_or(true),
            };

            if inclusive_start && !merged.inclusive_start {
                inclusive_start = false;
            }
            if inclusive_end && !merged.inclusive_end {
                inclusive_end = false;
            }
            opts_list.push(merged);
        }

        let start_keys: Vec<Value> = opts_list.iter().map(|o| o.start_key.clone()).collect();
        let end_keys: Vec<Value> = opts_list.iter().map(|o| o.end_key.clone()).collect();
        let index_filtered_join = index
            .iter()
            .filter(|f| !sort_irrelevant_fields.contains(*f))
            .cloned()
            .collect::<Vec<_>>()
            .join(",");

        let query_plan = RxQueryPlan {
            index: index.clone(),
            start_keys: start_keys.clone(),
            end_keys: end_keys.clone(),
            inclusive_end,
            inclusive_start,
            sort_satisfied_by_index: !has_desc_sorting
                && optimal_sort_index_compare_string == index_filtered_join,
            selector_satisfied_by_index: is_selector_satisfied_by_index(
                index,
                selector,
                &start_keys,
                &end_keys,
            ),
        };
        let quality = rate_query_plan(schema, query, &query_plan);
        if quality >= current_best_quality || query.index.is_some() {
            current_best_quality = quality;
            current_best_query_plan = Some(query_plan);
        }
    }

    current_best_query_plan.ok_or_else(|| {
        new_rx_error(
            "SNH",
            Some(json!({ "query": serde_json::to_value(query).unwrap_or(Value::Null) })),
        )
    })
}

// ref: rxdb/src/query-planner.ts:180-306
pub fn is_selector_satisfied_by_index(
    index: &[String],
    selector: &MangoQuerySelector,
    start_keys: &[RxQueryPlanKey],
    end_keys: &[RxQueryPlanKey],
) -> bool {
    let sel_obj = match selector.as_object() {
        Some(o) => o,
        None => return false,
    };

    // Not satisfied if one or more operators are non-logical
    // operators that can never be satisfied by an index.
    for (field_name, operation) in sel_obj.iter() {
        if !index.iter().any(|f| f == field_name) {
            return false;
        }
        if let Some(op_obj) = operation.as_object() {
            for op in op_obj.keys() {
                if !LOGICAL_OPERATORS.contains(op.as_str()) {
                    return false;
                }
            }
        }
    }

    // Not satisfied if contains $and or $or operations.
    if sel_obj.contains_key("$and") || sel_obj.contains_key("$or") {
        return false;
    }

    // ensure all lower bound in index
    let mut satisfied_lower_bound: Vec<String> = Vec::new();
    let mut lower_operator_field_names: HashSet<String> = HashSet::new();
    for (field_name, operation) in sel_obj.iter() {
        if !index.iter().any(|f| f == field_name) {
            return false;
        }
        let op_keys: Vec<String> = operation
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        let lower_logic_ops: Vec<&String> = op_keys
            .iter()
            .filter(|k| LOWER_BOUND_LOGICAL_OPERATORS.contains(k.as_str()))
            .collect();
        if lower_logic_ops.len() > 1 {
            return false;
        }
        if let Some(op) = lower_logic_ops.first() {
            lower_operator_field_names.insert(field_name.clone());
            if op.as_str() != "$eq" {
                if !satisfied_lower_bound.is_empty() {
                    return false;
                }
                satisfied_lower_bound.push((*op).clone());
            }
        }
    }

    // ensure all upper bound in index
    let mut satisfied_upper_bound: Vec<String> = Vec::new();
    let mut upper_operator_field_names: HashSet<String> = HashSet::new();
    for (field_name, operation) in sel_obj.iter() {
        if !index.iter().any(|f| f == field_name) {
            return false;
        }
        let op_keys: Vec<String> = operation
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        let upper_logic_ops: Vec<&String> = op_keys
            .iter()
            .filter(|k| UPPER_BOUND_LOGICAL_OPERATORS.contains(k.as_str()))
            .collect();
        if upper_logic_ops.len() > 1 {
            return false;
        }
        if let Some(op) = upper_logic_ops.first() {
            upper_operator_field_names.insert(field_name.clone());
            if op.as_str() != "$eq" {
                if !satisfied_upper_bound.is_empty() {
                    return false;
                }
                satisfied_upper_bound.push((*op).clone());
            }
        }
    }

    // If the index contains a non-relevant field between the relevant fields,
    // then the index is not satisfying.
    let mut i = 0usize;
    for field_name in index.iter() {
        for set in [
            &mut lower_operator_field_names,
            &mut upper_operator_field_names,
        ] {
            if !set.contains(field_name) && !set.is_empty() {
                return false;
            }
            set.remove(field_name);
        }
        let start_key = &start_keys[i];
        let end_key = &end_keys[i];
        if start_key != end_key
            && !lower_operator_field_names.is_empty()
            && !upper_operator_field_names.is_empty()
        {
            return false;
        }
        i += 1;
    }
    true
}

// ref: rxdb/src/query-planner.ts:308-343
pub fn get_matcher_query_opts(
    operator: &str,
    operator_value: &Value,
) -> Result<PartialRxQueryPlanerOpts, RxError> {
    Ok(match operator {
        "$eq" => PartialRxQueryPlanerOpts {
            start_key: Some(operator_value.clone()),
            end_key: Some(operator_value.clone()),
            inclusive_start: Some(true),
            inclusive_end: Some(true),
        },
        "$lte" => PartialRxQueryPlanerOpts {
            end_key: Some(operator_value.clone()),
            inclusive_end: Some(true),
            ..Default::default()
        },
        "$gte" => PartialRxQueryPlanerOpts {
            start_key: Some(operator_value.clone()),
            inclusive_start: Some(true),
            ..Default::default()
        },
        "$lt" => PartialRxQueryPlanerOpts {
            end_key: Some(operator_value.clone()),
            inclusive_end: Some(false),
            ..Default::default()
        },
        "$gt" => PartialRxQueryPlanerOpts {
            start_key: Some(operator_value.clone()),
            inclusive_start: Some(false),
            ..Default::default()
        },
        _ => return Err(new_rx_error("SNH", Some(json!({ "operator": operator })))),
    })
}

// ref: rxdb/src/query-planner.ts:350-383
/// Returns a number that determines the quality of the query plan.
/// Higher number means better query plan.
pub fn rate_query_plan(
    _schema: &RxJsonSchema,
    _query: &FilledMangoQuery,
    query_plan: &RxQueryPlan,
) -> f64 {
    let mut quality: f64 = 0.0;
    let mut add_quality = |value: f64| {
        if value > 0.0 {
            quality += value;
        }
    };
    let points_per_matching_key = 10.0;

    let non_min_key_count = count_until_not_matching(&query_plan.start_keys, |kv, _| {
        kv != &*INDEX_MIN && kv != &*INDEX_MAX
    });
    add_quality(non_min_key_count as f64 * points_per_matching_key);

    let non_max_key_count = count_until_not_matching(&query_plan.start_keys, |kv, _| {
        kv != &*INDEX_MAX && kv != &*INDEX_MIN
    });
    add_quality(non_max_key_count as f64 * points_per_matching_key);

    let equal_key_count = count_until_not_matching(&query_plan.start_keys, |kv, idx| {
        kv == &query_plan.end_keys[idx]
    });
    add_quality(equal_key_count as f64 * points_per_matching_key * 1.5);

    let points_if_no_resort_must_be_done = if query_plan.sort_satisfied_by_index {
        5.0
    } else {
        0.0
    };
    add_quality(points_if_no_resort_must_be_done);
    quality
}
