//! Helpers for `RxJsonSchema` manipulation.
//!
//! T1/T3 notes:
//! - Upstream is generic over `RxDocType`; the Rust port operates on the
//!   dynamic [`RxJsonSchema`] struct and `serde_json::Value` documents.
//! - Object-default filling is exposed at the collection-helper layer where the
//!   Rust port has both schema defaults and insert-envelope context available.

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::plugins::utils::utils_array::append_to_array;
use crate::plugins::utils::utils_document::RX_META_LWT_MINIMUM;
use crate::plugins::utils::utils_object::sort_object;
use crate::plugins::utils::utils_object_dot_prop::get_property;
use crate::plugins::utils::utils_other::ensure_not_falsy;
use crate::plugins::utils::utils_string::trim_dots;
use crate::rx_error::{new_rx_error, RxResult};
use crate::types::{
    CompositePrimaryKey, JsonSchema, PrimaryKey, RxJsonSchema, RxStorageDefaultCheckpoint,
};

// ref: rxdb/src/rx-schema-helper.ts:29-52
/// Helper function to create a valid RxJsonSchema with a given version.
pub fn get_pseudo_schema_for_version(version: i32, primary_key: &str) -> RxJsonSchema {
    let mut properties = std::collections::HashMap::new();
    properties.insert(
        primary_key.to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            max_length: Some(100),
            ..Default::default()
        },
    );
    properties.insert(
        "value".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            ..Default::default()
        },
    );
    let base = RxJsonSchema {
        version,
        primary_key: PrimaryKey::Simple(primary_key.to_string()),
        schema_type: "object".to_string(),
        properties,
        required: vec![primary_key.to_string()],
        indexes: vec![vec![primary_key.to_string()]],
        encrypted: Vec::new(),
        internal_indexes: Vec::new(),
        key_compression: false,
        attachments: None,
        additional_properties: false,
        extra: std::collections::HashMap::new(),
    };
    fill_with_default_settings(base)
}

// ref: rxdb/src/rx-schema-helper.ts:57-68
/// Returns the sub-schema for a given dotted path.
pub fn get_schema_by_object_path(rx_json_schema: &RxJsonSchema, path: &str) -> JsonSchema {
    let mut use_path = path.replace('.', ".properties.");
    use_path = format!("properties.{use_path}");
    use_path = trim_dots(&use_path);
    let as_value = serde_json::to_value(rx_json_schema).unwrap_or(Value::Null);
    let ret = get_property(&as_value, &use_path, None);
    serde_json::from_value(ret).unwrap_or_default()
}

pub fn has_schema_object_path(rx_json_schema: &RxJsonSchema, path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let mut current = &rx_json_schema.properties;
    let mut current_schema: Option<&JsonSchema> = None;
    for part in path.split('.') {
        let Some(schema_part) = current.get(part) else {
            return false;
        };
        current_schema = Some(schema_part);
        current = &schema_part.properties;
    }
    current_schema.is_some()
}

// ref: rxdb/src/rx-schema-helper.ts:70-103
pub fn fill_primary_key(
    primary_path: &str,
    json_schema: &RxJsonSchema,
    document_data: &mut Value,
) -> RxResult<()> {
    // optimization shortcut: a simple-string primaryKey needs no recomputation.
    if matches!(json_schema.primary_key, PrimaryKey::Simple(_)) {
        return Ok(());
    }
    let new_primary = get_composed_primary_key_of_document_data(json_schema, document_data)?;
    if let Some(existing) = document_data.get(primary_path).and_then(|v| v.as_str()) {
        if !existing.is_empty() && existing != new_primary {
            return Err(new_rx_error(
                "DOC19",
                Some(json!({
                    "args": {
                        "documentData": document_data,
                        "existingPrimary": existing,
                        "newPrimary": new_primary,
                    },
                    "schema": serde_json::to_value(json_schema).unwrap_or(Value::Null),
                })),
            ));
        }
    }
    if let Some(obj) = document_data.as_object_mut() {
        obj.insert(primary_path.to_string(), Value::String(new_primary));
    }
    Ok(())
}

// ref: rxdb/src/rx-schema-helper.ts:105-113
pub fn get_primary_field_of_primary_key(primary_key: &PrimaryKey) -> String {
    primary_key.primary_field().to_string()
}

// ref: rxdb/src/rx-schema-helper.ts:115-121
pub fn get_length_of_primary_key(schema: &RxJsonSchema) -> RxResult<u64> {
    let primary_path = get_primary_field_of_primary_key(&schema.primary_key);
    let schema_part = get_schema_by_object_path(schema, &primary_path);
    ensure_not_falsy(
        schema_part.max_length,
        Some("missing maxLength on primary key"),
    )
}

// ref: rxdb/src/rx-schema-helper.ts:126-142
/// Returns the composed primaryKey of a document by its data.
pub fn get_composed_primary_key_of_document_data(
    json_schema: &RxJsonSchema,
    document_data: &Value,
) -> RxResult<String> {
    match &json_schema.primary_key {
        PrimaryKey::Simple(s) => Ok(document_data
            .get(s)
            .and_then(|v| {
                v.as_str()
                    .map(|x| x.to_string())
                    .or_else(|| Some(v.to_string()))
            })
            .unwrap_or_default()),
        PrimaryKey::Composite(CompositePrimaryKey {
            fields, separator, ..
        }) => {
            let mut parts = Vec::with_capacity(fields.len());
            for field in fields {
                let value = get_property(document_data, field, None);
                if value.is_null() {
                    return Err(new_rx_error(
                        "DOC18",
                        Some(json!({ "args": { "field": field, "documentData": document_data } })),
                    ));
                }
                parts.push(match &value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                });
            }
            Ok(parts.join(separator))
        }
    }
}

// ref: rxdb/src/rx-schema-helper.ts:157-160
/// Normalize the RxJsonSchema.
/// - Orders the schemas attributes by alphabetical order
/// - Adds the primaryKey to all indexes that do not contain the primaryKey
///   (handled by `fill_with_default_settings` instead, called separately upstream)
pub fn normalize_rx_json_schema(json_schema: &RxJsonSchema) -> RxJsonSchema {
    let as_value = serde_json::to_value(json_schema).unwrap_or(Value::Null);
    let sorted = sort_object(&as_value, true);
    serde_json::from_value(sorted).unwrap_or_else(|_| json_schema.clone())
}

// ref: rxdb/src/rx-schema-helper.ts:167-169
/// If the schema does not specify any index,
/// we add this index so we at least can run RxQuery()
/// and only select non-deleted fields.
pub fn get_default_index(primary_path: &str) -> Vec<String> {
    vec!["_deleted".to_string(), primary_path.to_string()]
}

// ref: rxdb/src/rx-schema-helper.ts:175-286
/// fills the schema-json with default-settings
pub fn fill_with_default_settings(mut schema_obj: RxJsonSchema) -> RxJsonSchema {
    let primary_path = get_primary_field_of_primary_key(&schema_obj.primary_key);

    // additionalProperties is always false
    schema_obj.additional_properties = false;

    // fill with key-compression-state — already typed as `bool` (defaults to false on deserialize).

    // _rev / _attachments / _deleted / _meta properties (always present)
    schema_obj.properties.insert(
        "_rev".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            min_length: Some(1),
            ..Default::default()
        },
    );
    schema_obj.properties.insert(
        "_attachments".to_string(),
        JsonSchema {
            schema_type: Some("object".to_string()),
            ..Default::default()
        },
    );
    schema_obj.properties.insert(
        "_deleted".to_string(),
        JsonSchema {
            schema_type: Some("boolean".to_string()),
            ..Default::default()
        },
    );
    schema_obj
        .properties
        .insert("_meta".to_string(), rx_meta_schema());

    // meta fields are all required
    let meta_required = ["_deleted", "_rev", "_meta", "_attachments"];
    for m in meta_required.iter() {
        if !schema_obj.required.iter().any(|r| r == m) {
            schema_obj.required.push((*m).to_string());
        }
    }

    // final fields are always required
    let final_fields = get_final_fields(&schema_obj);
    append_to_array(&mut schema_obj.required, &final_fields);
    // strip dotted paths and dedup
    let mut seen = HashSet::new();
    schema_obj
        .required
        .retain(|f| !f.contains('.') && seen.insert(f.clone()));

    // indexes
    let mut use_indexes: Vec<Vec<String>> = schema_obj
        .indexes
        .iter()
        .map(|index| {
            let mut ar = index.clone();
            // Append primary key to indexes that do not contain it.
            if !ar.contains(&primary_path) {
                ar.push(primary_path.clone());
            }
            // add _deleted flag to all indexes
            if ar.first().map(String::as_str) != Some("_deleted") {
                ar.insert(0, "_deleted".to_string());
            }
            ar
        })
        .collect();

    if use_indexes.is_empty() {
        use_indexes.push(get_default_index(&primary_path));
    }

    // we need this index for the getChangedDocumentsSince() method
    use_indexes.push(vec!["_meta.lwt".to_string(), primary_path.clone()]);

    // also add the internalIndexes
    for idx in &schema_obj.internal_indexes {
        use_indexes.push(idx.clone());
    }

    // make indexes unique
    let mut has_index = HashSet::new();
    use_indexes.retain(|idx| {
        let s = idx.join(",");
        has_index.insert(s)
    });
    schema_obj.indexes = use_indexes;

    schema_obj
}

// ref: rxdb/src/rx-schema-helper.ts:288
pub const META_LWT_UNIX_TIME_MAX: f64 = 1_000_000_000_000_000.0;

// ref: rxdb/src/rx-schema-helper.ts:289-314
/// Schema for the `_meta` property RxDB attaches to every document.
pub fn rx_meta_schema() -> JsonSchema {
    let mut properties = std::collections::HashMap::new();
    properties.insert(
        "lwt".to_string(),
        JsonSchema {
            schema_type: Some("number".to_string()),
            minimum: Some(RX_META_LWT_MINIMUM as f64),
            maximum: Some(META_LWT_UNIX_TIME_MAX),
            multiple_of: Some(0.01),
            ..Default::default()
        },
    );
    JsonSchema {
        schema_type: Some("object".to_string()),
        properties,
        required: vec!["lwt".to_string()],
        // Additional properties allowed (upstream `additionalProperties: true`).
        additional_properties: Some(true),
        ..Default::default()
    }
}

// ref: rxdb/src/rx-schema-helper.ts:321-338
/// returns the final-field names of the schema.
pub fn get_final_fields(json_schema: &RxJsonSchema) -> Vec<String> {
    let mut ret: Vec<String> = json_schema
        .properties
        .iter()
        .filter(|(_, v)| v.final_field == Some(true))
        .map(|(k, _)| k.clone())
        .collect();
    // primary is also final
    let primary_path = get_primary_field_of_primary_key(&json_schema.primary_key);
    ret.push(primary_path);
    // fields of composite primary are final
    if let PrimaryKey::Composite(c) = &json_schema.primary_key {
        for field in &c.fields {
            ret.push(field.clone());
        }
    }
    ret
}

// ref: rxdb/src/rx-schema-helper.ts:344-353
// `fillObjectWithDefaults(rxSchema, obj)` is implemented as
// `rx_collection_helper::fill_object_data_before_insert`, where the Rust port
// also adds `_meta`, `_deleted`, `_attachments`, `_rev`, and composite primary
// values before storage writes.

// ref: rxdb/src/rx-schema-helper.ts:355-370
/// Default checkpoint schema for storages.
pub fn default_checkpoint_schema() -> JsonSchema {
    let mut properties = std::collections::HashMap::new();
    properties.insert(
        "id".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            ..Default::default()
        },
    );
    properties.insert(
        "lwt".to_string(),
        JsonSchema {
            schema_type: Some("number".to_string()),
            ..Default::default()
        },
    );
    JsonSchema {
        schema_type: Some("object".to_string()),
        properties,
        required: vec!["id".to_string(), "lwt".to_string()],
        additional_properties: Some(false),
        ..Default::default()
    }
}

/// Helper construction of a [`RxStorageDefaultCheckpoint`].
pub fn default_checkpoint(id: impl Into<String>, lwt: f64) -> RxStorageDefaultCheckpoint {
    RxStorageDefaultCheckpoint { id: id.into(), lwt }
}
