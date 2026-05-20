//! Port of `src/rx-change-event.ts`.
//!
//! T1 deviations:
//! - The `rxChangeEventBulkToRxChangeEvents` cache (upstream's `EVENT_BULK_CACHE`
//!   WeakMap) is omitted; callers that need memoization can hold the result
//!   themselves. Bulk-event materialization is cheap.
//! - `rxChangeEventToEventReduceChangeEvent` produces the JSON-shape that
//!   upstream's `event-reduce-js` consumes; that crate is skipped in our MVP
//!   scope (see N15) so this function is preserved for API parity only.

use serde_json::{json, Value};

use crate::overwritable::OVERWRITABLE;
use crate::types::{RxChangeEvent, RxChangeEventBulk, RxStorageChangeEvent};

// ref: rxdb/src/rx-change-event.ts:20-28
pub fn get_document_data_of_rx_change_event(rx_change_event: &RxStorageChangeEvent) -> Value {
    if let Some(data) = &rx_change_event.document_data {
        data.clone()
    } else {
        rx_change_event
            .previous_document_data
            .clone()
            .unwrap_or(Value::Null)
    }
}

// ref: rxdb/src/rx-change-event.ts:36-62
/// Convert to the `event-reduce-js` JSON shape. Returns `None` if the event
/// is not relevant for event-reduce (e.g. update of an already-deleted doc).
pub fn rx_change_event_to_event_reduce_change_event(
    rx_change_event: &RxStorageChangeEvent,
) -> Option<Value> {
    match rx_change_event.operation.as_str() {
        "INSERT" => Some(json!({
            "operation": rx_change_event.operation,
            "id": rx_change_event.document_id,
            "doc": rx_change_event.document_data.clone().unwrap_or(Value::Null),
            "previous": Value::Null,
        })),
        "UPDATE" => {
            let doc = rx_change_event.document_data.clone().unwrap_or(Value::Null);
            let frozen = (OVERWRITABLE.load().deep_freeze_when_dev_mode)(doc);
            let previous = match &rx_change_event.previous_document_data {
                Some(p) => p.clone(),
                None => Value::String("UNKNOWN".to_string()),
            };
            Some(json!({
                "operation": rx_change_event.operation,
                "id": rx_change_event.document_id,
                "doc": frozen,
                "previous": previous,
            }))
        }
        "DELETE" => Some(json!({
            "operation": rx_change_event.operation,
            "id": rx_change_event.document_id,
            "doc": Value::Null,
            "previous": rx_change_event.previous_document_data.clone().unwrap_or(Value::Null),
        })),
        _ => None,
    }
}

// ref: rxdb/src/rx-change-event.ts:68-108
/// Flattens nested event-bulk structures into a deduplicated event list.
/// Upstream walks an `EventBulk<E, _> | EventBulk[] | E | E[]` recursively
/// and dedups by `(documentId, doc._rev, previousDoc._rev)`. The Rust port
/// accepts already-flattened input (`&[RxStorageChangeEvent]`) and just
/// dedups; CTOX call sites already have flat lists.
pub fn flatten_events(input: &[RxStorageChangeEvent]) -> Vec<RxStorageChangeEvent> {
    let mut used_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(input.len());
    for ev in input.iter() {
        let id = format!(
            "{}|{}|{}",
            ev.document_id,
            ev.document_data
                .as_ref()
                .and_then(|d| d.get("_rev"))
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            ev.previous_document_data
                .as_ref()
                .and_then(|d| d.get("_rev"))
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        );
        if used_ids.insert(id) {
            out.push(ev.clone());
        }
    }
    out
}

// ref: rxdb/src/rx-change-event.ts:110-137
/// Convert a storage-level `RxChangeEventBulk` into per-event `RxChangeEvent`s.
pub fn rx_change_event_bulk_to_rx_change_events(
    event_bulk: &RxChangeEventBulk,
) -> Vec<RxChangeEvent> {
    let deep_freeze = OVERWRITABLE.load().deep_freeze_when_dev_mode.clone();
    event_bulk
        .events
        .iter()
        .map(|event| {
            let doc_data = event.document_data.clone();
            let prev_data = event.previous_document_data.clone();
            RxChangeEvent {
                document_id: event.document_id.clone(),
                collection_name: event_bulk.collection_name.clone(),
                is_local: event_bulk.is_local,
                operation: event.operation.clone(),
                document_data: doc_data.map(|d| (deep_freeze)(d)),
                previous_document_data: prev_data.map(|d| (deep_freeze)(d)),
            }
        })
        .collect()
}
