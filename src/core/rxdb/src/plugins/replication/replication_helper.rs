//! Port of `src/plugins/replication/replication-helper.ts`.
//!
//! `await_retry` and `prevent_hibernate_browser_tab` intentionally differ from
//! browser RxDB: CTOX runs server-side, so the browser-only online-event
//! short-circuit and synthetic `mousemove` hibernation workaround collapse to a
//! process-local sleep/no-op.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde_json::Value;

use crate::plugins::utils::utils_object::flat_clone;
use crate::rx_collection::RxCollection;
use crate::rx_error::RxResult;

// ref: rxdb/src/plugins/replication/replication-helper.ts:10
/// Default modifier — passes the document through unchanged.
pub fn default_modifier(d: Value) -> Pin<Box<dyn Future<Output = RxResult<Value>> + Send>> {
    Box::pin(async move { Ok(d) })
}

// ref: rxdb/src/plugins/replication/replication-helper.ts:13-26
/// Swap the canonical `_deleted` field for a custom deleted-field name on the
/// document being sent to the remote.
pub fn swap_default_deleted_to_deleted_field(deleted_field: &str, doc: &Value) -> Value {
    if deleted_field == "_deleted" {
        return doc.clone();
    }
    let mut copy = flat_clone(doc);
    if let Some(obj) = copy.as_object_mut() {
        let is_deleted = obj
            .get("_deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        obj.insert(deleted_field.to_string(), Value::Bool(is_deleted));
        obj.remove("_deleted");
    }
    copy
}

// ref: rxdb/src/plugins/replication/replication-helper.ts:33-63
pub fn handle_pulled_documents(
    collection: &RxCollection,
    deleted_field: &str,
    docs: Vec<Value>,
) -> RxResult<Vec<Value>> {
    let schema = collection.schema_required()?;
    handle_pulled_documents_with_schema(&schema.json_schema, deleted_field, docs)
}

/// Variant of upstream `handle_pulled_documents` that takes the schema
/// directly instead of an `RxCollection` handle. Operates on `Value` docs.
pub fn handle_pulled_documents_with_schema(
    schema: &crate::types::RxJsonSchema,
    deleted_field: &str,
    docs: Vec<Value>,
) -> RxResult<Vec<Value>> {
    use crate::rx_schema_helper::{
        get_composed_primary_key_of_document_data, get_primary_field_of_primary_key,
    };
    let primary_path = get_primary_field_of_primary_key(&schema.primary_key);
    let mut out = Vec::with_capacity(docs.len());
    for doc in docs.iter() {
        let mut use_doc = flat_clone(doc);
        if let Some(obj) = use_doc.as_object_mut() {
            // Swap out the deleted field.
            if deleted_field != "_deleted" {
                let is_deleted = obj
                    .get(deleted_field)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                obj.insert("_deleted".to_string(), Value::Bool(is_deleted));
                obj.remove(deleted_field);
            } else {
                let is_deleted = obj
                    .get("_deleted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                obj.insert("_deleted".to_string(), Value::Bool(is_deleted));
            }
        }
        // Fill composed primary key.
        let composed = get_composed_primary_key_of_document_data(schema, &use_doc)?;
        if let Some(obj) = use_doc.as_object_mut() {
            obj.insert(primary_path.clone(), Value::String(composed));
        }
        out.push(use_doc);
    }
    Ok(out)
}

// ref: rxdb/src/plugins/replication/replication-helper.ts:70-98
/// Like a normal `promise_wait`, but designed to be short-circuited by an
/// "online" event in the browser. In CTOX we are always "online" from the
/// process's perspective, so we just wait the retry time.
pub async fn await_retry(retry_time_ms: u64) {
    tokio::time::sleep(Duration::from_millis(retry_time_ms)).await;
}

// ref: rxdb/src/plugins/replication/replication-helper.ts:108-122
// `prevent_hibernate_browser_tab` is a browser-tab hibernation workaround
// (dispatches mousemove events). CTOX runs server-side and has no browser
// context — this is an intentional no-op.
pub fn prevent_hibernate_browser_tab() {}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;
    use crate::types::{CompositePrimaryKey, JsonSchema, PrimaryKey, RxJsonSchema};

    fn composite_schema() -> RxJsonSchema {
        let mut properties = HashMap::new();
        properties.insert(
            "id".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                ..Default::default()
            },
        );
        properties.insert(
            "first".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                ..Default::default()
            },
        );
        properties.insert(
            "last".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                ..Default::default()
            },
        );
        RxJsonSchema {
            version: 0,
            primary_key: PrimaryKey::Composite(CompositePrimaryKey {
                key: "id".to_string(),
                fields: vec!["first".to_string(), "last".to_string()],
                separator: "|".to_string(),
            }),
            schema_type: "object".to_string(),
            properties,
            required: vec!["id".to_string(), "first".to_string(), "last".to_string()],
            indexes: Vec::new(),
            encrypted: Vec::new(),
            internal_indexes: Vec::new(),
            key_compression: false,
            attachments: None,
            additional_properties: false,
            extra: HashMap::new(),
        }
    }

    #[test]
    fn handle_pulled_documents_swaps_deleted_and_fills_composite_primary() {
        let docs = handle_pulled_documents_with_schema(
            &composite_schema(),
            "deleted",
            vec![json!({
                "first": "Ada",
                "last": "Lovelace",
                "deleted": true,
            })],
        )
        .unwrap();

        assert_eq!(docs[0]["id"], json!("Ada|Lovelace"));
        assert_eq!(docs[0]["_deleted"], json!(true));
        assert!(docs[0].get("deleted").is_none());
    }

    #[test]
    fn swap_default_deleted_to_deleted_field_preserves_default_shape() {
        let doc = json!({ "id": "a", "_deleted": true });

        assert_eq!(swap_default_deleted_to_deleted_field("_deleted", &doc), doc);
        assert_eq!(
            swap_default_deleted_to_deleted_field("deleted", &doc),
            json!({ "id": "a", "deleted": true })
        );
    }
}
