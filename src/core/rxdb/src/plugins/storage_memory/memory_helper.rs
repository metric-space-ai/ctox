//! Port of `src/plugins/storage-memory/memory-helper.ts`.
//!
//! T1 deviations:
//! - `ensure_not_removed` takes the internals state + `(database_name, collection_name)`
//!   instead of a forward-declared `RxStorageInstanceMemory` reference. Same
//!   behaviour, different argument shape.
//! - The `array-push-at-sort-position` NPM dependency is inlined as
//!   [`push_at_sort_position`] using `Vec::binary_search_by` + `Vec::insert`.

use std::cmp::Ordering;

use serde_json::{json, Value};

use crate::plugins::storage_memory::binary_search_bounds::bound_eq;
use crate::plugins::storage_memory::memory_types::{
    DocWithIndexString, MemoryStorageInternals, MemoryStorageInternalsByIndex,
};
use crate::rx_error::{new_rx_error, RxResult};
use crate::rx_schema_helper::get_primary_field_of_primary_key;
use crate::types::RxJsonSchema;

// ref: rxdb/src/plugins/storage-memory/memory-helper.ts:19-29
pub fn get_memory_collection_key(
    database_name: &str,
    collection_name: &str,
    schema_version: i32,
) -> String {
    format!("{database_name}--memory--{collection_name}--memory--{schema_version}")
}

// ref: rxdb/src/plugins/storage-memory/memory-helper.ts:32-42
pub fn ensure_not_removed(
    state: &MemoryStorageInternals,
    database_name: &str,
    collection_name: &str,
) -> RxResult<()> {
    if state.removed {
        return Err(new_rx_error(
            "MS1",
            Some(json!({
                "message": format!(
                    "removed already {database_name} - {collection_name} - {}",
                    state.schema.version
                ),
            })),
        ));
    }
    Ok(())
}

// ref: rxdb/src/plugins/storage-memory/memory-helper.ts:44-46
pub fn attachment_map_key(document_id: &str, attachment_id: &str) -> String {
    format!("{document_id}||{attachment_id}")
}

// ref: rxdb/src/plugins/storage-memory/memory-helper.ts:49-55
fn sort_by_index_string_comparator(a: &DocWithIndexString, b: &DocWithIndexString) -> Ordering {
    if a.index_string < b.index_string {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

/// Inlined port of the `array-push-at-sort-position` NPM package.
/// Returns the insertion index.
fn push_at_sort_position<T>(
    arr: &mut Vec<T>,
    item: T,
    comparator: impl Fn(&T, &T) -> Ordering,
    start_index: usize,
) -> usize {
    let slice = &arr[start_index..];
    let rel_pos = slice
        .binary_search_by(|x| comparator(x, &item))
        .unwrap_or_else(|e| e);
    let pos = rel_pos + start_index;
    arr.insert(pos, item);
    pos
}

// ref: rxdb/src/plugins/storage-memory/memory-helper.ts:62-127
/// @hotPath in upstream.
pub fn put_write_row_to_state(
    doc_id: &str,
    state: &mut MemoryStorageInternals,
    state_by_index: &mut [&mut MemoryStorageInternalsByIndex],
    document: Value,
    doc_in_state: Option<&Value>,
) -> RxResult<()> {
    state.documents.insert(doc_id.to_string(), document.clone());
    for by_index in state_by_index.iter_mut() {
        let new_index_string = (by_index.get_indexable_string)(&document);
        let insert_position = push_at_sort_position(
            &mut by_index.docs_with_index,
            DocWithIndexString {
                index_string: new_index_string.clone(),
                document: document.clone(),
                id: doc_id.to_string(),
            },
            sort_by_index_string_comparator,
            0,
        );

        // Remove previous if it was in the state.
        if let Some(prev_doc) = doc_in_state {
            let previous_index_string = (by_index.get_indexable_string)(prev_doc);
            if previous_index_string == new_index_string {
                // Performance shortcut: if index was not changed, the old doc
                // must be at insert_position - 1 or insert_position + 1.
                let docs = &mut by_index.docs_with_index;
                let prev_idx = insert_position.checked_sub(1);
                if let Some(p) = prev_idx {
                    if docs.get(p).map(|d| d.id.as_str()) == Some(doc_id) {
                        docs.remove(p);
                        continue;
                    }
                }
                let next_idx = insert_position + 1;
                if docs.get(next_idx).map(|d| d.id.as_str()) == Some(doc_id) {
                    docs.remove(next_idx);
                    continue;
                }
                return Err(new_rx_error(
                    "SNH",
                    Some(json!({
                        "document": document,
                        "args": { "byIndex": by_index.index },
                    })),
                ));
            } else {
                // Index changed, search for the old one and remove it.
                let probe = DocWithIndexString {
                    index_string: previous_index_string,
                    document: Value::Null,
                    id: String::new(),
                };
                let index_before = bound_eq(
                    &by_index.docs_with_index,
                    &probe,
                    &(|a, b| {
                        if a.index_string < b.index_string {
                            Ordering::Less
                        } else if a.index_string == b.index_string {
                            Ordering::Equal
                        } else {
                            Ordering::Greater
                        }
                    }),
                    None,
                    None,
                );
                if index_before >= 0 {
                    by_index.docs_with_index.remove(index_before as usize);
                }
            }
        }
    }
    Ok(())
}

// ref: rxdb/src/plugins/storage-memory/memory-helper.ts:130-152
pub fn remove_doc_from_state(
    primary_path: &str,
    _schema: &RxJsonSchema,
    state: &mut MemoryStorageInternals,
    doc: &Value,
) {
    let doc_id = doc
        .get(primary_path)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    state.documents.remove(&doc_id);

    for by_index in state.by_index.values_mut() {
        let index_string = (by_index.get_indexable_string)(doc);
        let probe = DocWithIndexString {
            index_string,
            document: Value::Null,
            id: String::new(),
        };
        let pos = bound_eq(
            &by_index.docs_with_index,
            &probe,
            &compare_docs_with_index,
            None,
            None,
        );
        if pos >= 0 {
            by_index.docs_with_index.remove(pos as usize);
        }
    }
}

// ref: rxdb/src/plugins/storage-memory/memory-helper.ts:155-168
pub fn compare_docs_with_index(a: &DocWithIndexString, b: &DocWithIndexString) -> Ordering {
    if a.index_string < b.index_string {
        Ordering::Less
    } else if a.index_string == b.index_string {
        Ordering::Equal
    } else {
        Ordering::Greater
    }
}

/// Convenience helper for use with the primary-key resolver elsewhere.
pub fn primary_path_of(schema: &RxJsonSchema) -> String {
    get_primary_field_of_primary_key(&schema.primary_key)
}
