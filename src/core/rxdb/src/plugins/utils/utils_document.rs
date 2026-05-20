//! Document-shape helpers.
//!
//! Depends on [`utils_object::flat_clone`].

use std::cmp::Ordering;

use serde_json::Value;

use crate::plugins::utils::utils_object::flat_clone;

// ref: rxdb/src/plugins/utils/utils-document.ts:15
/// We use 1 as minimum so that the value is never falsy.
/// This const is used in several places because querying
/// with a value lower than the minimum could give false results.
pub const RX_META_LWT_MINIMUM: i64 = 1;

// ref: rxdb/src/plugins/utils/utils-document.ts:17-27
pub fn get_default_rx_document_meta() -> Value {
    serde_json::json!({ "lwt": RX_META_LWT_MINIMUM })
}

// ref: rxdb/src/plugins/utils/utils-document.ts:34-41
/// Returns a revision that is not valid.
/// Use this to have correct typings
/// while the storage wrapper anyway will overwrite the revision.
pub fn get_default_revision() -> String {
    String::new()
}

// ref: rxdb/src/plugins/utils/utils-document.ts:44-50
pub fn strip_meta_data_from_document(doc_data: &Value) -> Value {
    let mut copy = doc_data.clone();
    if let Some(obj) = copy.as_object_mut() {
        obj.remove("_meta");
        obj.remove("_deleted");
        obj.remove("_rev");
    }
    copy
}

// ref: rxdb/src/plugins/utils/utils-document.ts:58-82
/// Faster way to check the equality of document lists
/// compared to doing a deep-equal.
/// Here we only check the ids and revisions.
pub fn are_rx_document_arrays_equal(primary_path: &str, ar1: &[Value], ar2: &[Value]) -> bool {
    if ar1.len() != ar2.len() {
        return false;
    }
    for (row1, row2) in ar1.iter().zip(ar2.iter()) {
        if row1.get(primary_path) != row2.get(primary_path) {
            return false;
        }
        if row1.get("_rev") != row2.get("_rev") {
            return false;
        }
        let lwt1 = row1.get("_meta").and_then(|m| m.get("lwt"));
        let lwt2 = row2.get("_meta").and_then(|m| m.get("lwt"));
        if lwt1 != lwt2 {
            return false;
        }
    }
    true
}

// ref: rxdb/src/plugins/utils/utils-document.ts:86-98
/// Returns a comparator that sorts documents by `_meta.lwt`, with ties broken by `primary_path`.
pub fn get_sort_documents_by_last_write_time_comparator(
    primary_path: String,
) -> impl Fn(&Value, &Value) -> Ordering {
    move |a, b| {
        let lwt_a = a
            .get("_meta")
            .and_then(|m| m.get("lwt"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let lwt_b = b
            .get("_meta")
            .and_then(|m| m.get("lwt"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if lwt_a == lwt_b {
            let pa = a.get(&primary_path);
            let pb = b.get(&primary_path);
            // Upstream JS uses raw `<` comparison; we coerce to string keys for stability.
            let pa_s = pa.map(value_sort_key).unwrap_or_default();
            let pb_s = pb.map(value_sort_key).unwrap_or_default();
            if pb_s < pa_s {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        } else {
            lwt_a.partial_cmp(&lwt_b).unwrap_or(Ordering::Equal)
        }
    }
}

/// Cheap stable sort key for an arbitrary `serde_json::Value`.
fn value_sort_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ref: rxdb/src/plugins/utils/utils-document.ts:99-104
pub fn sort_documents_by_last_write_time(primary_path: &str, mut docs: Vec<Value>) -> Vec<Value> {
    let cmp = get_sort_documents_by_last_write_time_comparator(primary_path.to_string());
    docs.sort_by(cmp);
    docs
}

// ref: rxdb/src/plugins/utils/utils-document.ts:107-117
pub fn to_with_deleted(doc_data: &Value) -> Value {
    let mut copy = flat_clone(doc_data);
    if let Some(obj) = copy.as_object_mut() {
        let deleted_truthy = obj
            .get("_deleted")
            .map(|v| !v.is_null() && v.as_bool() != Some(false))
            .unwrap_or(false);
        obj.insert("_deleted".to_string(), Value::Bool(deleted_truthy));
        obj.remove("_attachments");
        obj.remove("_meta");
        obj.remove("_rev");
    }
    copy
}
