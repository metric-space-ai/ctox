//! Port of `src/event-reduce.ts` (**stub**, gap-item N15).
//!
//! Upstream uses the `event-reduce-js` library to optimize cached
//! `RxQuery._result` updates: given a stream of `RxChangeEvent`s, it tries to
//! mutate the cached result in place rather than re-running the full mango
//! query. The optimization is opaque and tightly coupled to upstream's
//! `QueryParams`/`StateResolveFunction` machinery.
//!
//! CTOX does not embed `event-reduce-js`. Instead we port a conservative
//! subset that only avoids a full re-query when every missed change is known
//! to be irrelevant to the query. Any change that matched before or matches
//! after still falls back to storage re-execution, which preserves correctness.

use std::cmp::Ordering;

use serde_json::Value;

use crate::rx_error::RxResult;
use crate::types::RxStorageChangeEvent;

// ref: rxdb/src/event-reduce.ts:18-30
/// Result of an event-reduce attempt. `run_full_query_again = true` ⇒ the
/// caller must re-run the underlying storage query; otherwise `new_results`
/// holds the patched cache and `changed` indicates whether anything moved.
#[derive(Debug, Clone)]
pub struct EventReduceResult {
    pub run_full_query_again: bool,
    pub changed: bool,
    pub new_results: Vec<Value>,
}

// ref: rxdb/src/event-reduce.ts:115-173
/// Conservative event-reduce subset. It returns `run_full_query_again = false`
/// only when no change event can affect the current query result.
pub fn calculate_new_results<F>(
    previous_results: &[Value],
    change_events: &[RxStorageChangeEvent],
    mut document_matches: F,
) -> RxResult<EventReduceResult>
where
    F: FnMut(&Value) -> RxResult<bool>,
{
    for event in change_events {
        let did_match_before = event
            .previous_document_data
            .as_ref()
            .map(&mut document_matches)
            .transpose()?
            .unwrap_or(false);
        let does_match_now = event
            .document_data
            .as_ref()
            .map(&mut document_matches)
            .transpose()?
            .unwrap_or(false);
        if did_match_before || does_match_now {
            return Ok(EventReduceResult {
                run_full_query_again: true,
                changed: false,
                new_results: Vec::new(),
            });
        }
    }
    Ok(EventReduceResult {
        run_full_query_again: false,
        changed: false,
        new_results: previous_results.to_vec(),
    })
}

/// Patch-capable variant for query shapes where Rust can mirror the useful
/// event-reduce actions without pulling in `event-reduce-js`.
///
/// This is intentionally limited to unpaginated find queries. With no
/// skip/limit window, a matching insert/update/delete can be applied directly
/// to the cached full result set and then sorted with the query comparator.
pub fn calculate_patched_find_results<F, C>(
    previous_results: &[Value],
    change_events: &[RxStorageChangeEvent],
    mut document_matches: F,
    primary_path: &str,
    sort_comparator: C,
) -> RxResult<EventReduceResult>
where
    F: FnMut(&Value) -> RxResult<bool>,
    C: Fn(&Value, &Value) -> Ordering,
{
    let mut new_results = previous_results.to_vec();
    let mut changed = false;

    for event in change_events {
        let did_match_before = event
            .previous_document_data
            .as_ref()
            .map(&mut document_matches)
            .transpose()?
            .unwrap_or(false);
        let does_match_now = event
            .document_data
            .as_ref()
            .map(&mut document_matches)
            .transpose()?
            .unwrap_or(false);

        match (did_match_before, does_match_now) {
            (false, false) => {}
            (false, true) => {
                let Some(document) = event.document_data.as_ref() else {
                    return Ok(run_full_query_again());
                };
                upsert_result_doc(&mut new_results, primary_path, document)?;
                changed = true;
            }
            (true, false) => {
                let Some(document) = event
                    .previous_document_data
                    .as_ref()
                    .or(event.document_data.as_ref())
                else {
                    return Ok(run_full_query_again());
                };
                remove_result_doc(&mut new_results, primary_path, document)?;
                changed = true;
            }
            (true, true) => {
                let Some(document) = event.document_data.as_ref() else {
                    return Ok(run_full_query_again());
                };
                upsert_result_doc(&mut new_results, primary_path, document)?;
                changed = true;
            }
        }
    }

    if changed {
        new_results.sort_by(sort_comparator);
    }
    Ok(EventReduceResult {
        run_full_query_again: false,
        changed,
        new_results,
    })
}

fn run_full_query_again() -> EventReduceResult {
    EventReduceResult {
        run_full_query_again: true,
        changed: false,
        new_results: Vec::new(),
    }
}

fn doc_id<'a>(primary_path: &str, document: &'a Value) -> RxResult<&'a str> {
    document
        .get(primary_path)
        .and_then(Value::as_str)
        .ok_or_else(|| crate::rx_error::new_rx_error("SNH", None))
}

fn upsert_result_doc(
    results: &mut Vec<Value>,
    primary_path: &str,
    document: &Value,
) -> RxResult<()> {
    let id = doc_id(primary_path, document)?;
    if let Some(existing) = results
        .iter_mut()
        .find(|candidate| candidate.get(primary_path).and_then(Value::as_str) == Some(id))
    {
        *existing = document.clone();
    } else {
        results.push(document.clone());
    }
    Ok(())
}

fn remove_result_doc(
    results: &mut Vec<Value>,
    primary_path: &str,
    document: &Value,
) -> RxResult<()> {
    let id = doc_id(primary_path, document)?;
    results.retain(|candidate| candidate.get(primary_path).and_then(Value::as_str) != Some(id));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(
        previous_document_data: Option<Value>,
        document_data: Option<Value>,
    ) -> RxStorageChangeEvent {
        RxStorageChangeEvent {
            operation: "UPDATE".to_string(),
            document_id: "doc".to_string(),
            document_data,
            previous_document_data,
            is_local: false,
        }
    }

    #[test]
    fn irrelevant_changes_do_not_require_full_query() {
        let result = calculate_new_results(
            &[serde_json::json!({ "id": "a", "age": 10 })],
            &[event(
                Some(serde_json::json!({ "id": "b", "age": 1 })),
                Some(serde_json::json!({ "id": "b", "age": 2 })),
            )],
            |doc| Ok(doc.get("age").and_then(Value::as_i64).unwrap_or(0) >= 10),
        )
        .unwrap();

        assert!(!result.run_full_query_again);
        assert!(!result.changed);
        assert_eq!(
            result.new_results,
            vec![serde_json::json!({ "id": "a", "age": 10 })]
        );
    }

    #[test]
    fn potentially_relevant_changes_fall_back_to_full_query() {
        let result = calculate_new_results(
            &[],
            &[event(
                Some(serde_json::json!({ "id": "b", "age": 1 })),
                Some(serde_json::json!({ "id": "b", "age": 10 })),
            )],
            |doc| Ok(doc.get("age").and_then(Value::as_i64).unwrap_or(0) >= 10),
        )
        .unwrap();

        assert!(result.run_full_query_again);
    }

    #[test]
    fn patched_find_results_insert_update_remove_and_sort() {
        let result = calculate_patched_find_results(
            &[
                serde_json::json!({ "id": "a", "age": 10 }),
                serde_json::json!({ "id": "b", "age": 20 }),
            ],
            &[
                event(None, Some(serde_json::json!({ "id": "c", "age": 15 }))),
                event(
                    Some(serde_json::json!({ "id": "b", "age": 20 })),
                    Some(serde_json::json!({ "id": "b", "age": 25 })),
                ),
                event(
                    Some(serde_json::json!({ "id": "a", "age": 10 })),
                    Some(serde_json::json!({ "id": "a", "age": 1, "_deleted": true })),
                ),
            ],
            |doc| {
                Ok(!doc
                    .get("_deleted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    && doc.get("age").and_then(Value::as_i64).unwrap_or(0) >= 10)
            },
            "id",
            |a, b| {
                a.get("age")
                    .and_then(Value::as_i64)
                    .cmp(&b.get("age").and_then(Value::as_i64))
            },
        )
        .unwrap();

        assert!(!result.run_full_query_again);
        assert!(result.changed);
        assert_eq!(
            result.new_results,
            vec![
                serde_json::json!({ "id": "c", "age": 15 }),
                serde_json::json!({ "id": "b", "age": 25 })
            ]
        );
    }
}
