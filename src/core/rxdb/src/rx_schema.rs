//! Port of `src/rx-schema.ts`.
//!
//! T1 deviations (documented inline at each site):
//! - `overwriteGetterForCaching` (JS prototype trick) → `std::sync::OnceLock`.
//! - `getDocumentPrototype` (constructs a JS prototype with getters per schema
//!   field) has no Rust analogue and is omitted; the Rust port does not have a
//!   RxDocument-prototype concept (see `PORTING.md` — plugin.ts T1 decision).
//!   Document access in user code is by serde_json or per-collection structs.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::hooks::run_plugin_hooks;
use crate::overwritable::OVERWRITABLE;
use crate::plugins::utils::utils_object::{clone_deep, sort_object};
use crate::plugins::utils::utils_object_deep_equal::deep_equal;
use crate::rx_error::{new_rx_error, RxResult};
use crate::rx_schema_helper::{
    fill_with_default_settings, get_composed_primary_key_of_document_data, get_final_fields,
    get_primary_field_of_primary_key, normalize_rx_json_schema,
};
use crate::types::{RxJsonSchema, SharedHashFunction};

// ref: rxdb/src/rx-schema.ts:33-185
/// `RxSchema` wraps a normalized [`RxJsonSchema`] plus a few derived caches.
#[derive(Clone)]
pub struct RxSchema {
    /// All declared indexes (with `_deleted`/primary key fill-ins applied).
    pub indexes: Vec<Vec<String>>,
    /// Storage primary path (a top-level property name).
    pub primary_path: String,
    /// Final-field names (immutable after first write).
    pub final_fields: Vec<String>,
    /// The normalized, default-filled JSON schema.
    pub json_schema: RxJsonSchema,
    /// Hash function provided by the user (e.g. SHA-256).
    pub hash_function: SharedHashFunction,
    // T1: caches for `defaultValues` and `hash` getters. Upstream uses
    // `overwriteGetterForCaching`; in Rust we use `OnceLock`.
    default_values_cache: OnceLock<HashMap<String, Value>>,
}

impl std::fmt::Debug for RxSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RxSchema")
            .field("version", &self.version())
            .field("primary_path", &self.primary_path)
            .field("indexes", &self.indexes)
            .field("final_fields", &self.final_fields)
            .finish()
    }
}

impl RxSchema {
    // ref: rxdb/src/rx-schema.ts:38-57
    pub fn new(json_schema: RxJsonSchema, hash_function: SharedHashFunction) -> RxResult<Self> {
        let indexes = get_indexes(&json_schema);
        let primary_path = get_primary_field_of_primary_key(&json_schema.primary_key);

        // Many people accidentally put in wrong schema state without the dev-mode plugin,
        // so we need this check here even in non-dev-mode.
        let primary_prop = json_schema.properties.get(&primary_path);
        let has_max_length = primary_prop
            .map(|p| p.max_length.is_some())
            .unwrap_or(false);
        if !has_max_length {
            return Err(new_rx_error(
                "SC39",
                Some(json!({
                    "schema": serde_json::to_value(&json_schema).unwrap_or(Value::Null),
                })),
            ));
        }

        let final_fields = get_final_fields(&json_schema);

        Ok(Self {
            indexes,
            primary_path,
            final_fields,
            json_schema,
            hash_function,
            default_values_cache: OnceLock::new(),
        })
    }

    // ref: rxdb/src/rx-schema.ts:59-61
    pub fn version(&self) -> i32 {
        self.json_schema.version
    }

    // ref: rxdb/src/rx-schema.ts:63-74
    /// Returns the default-values map for this schema (cached on first call).
    pub fn default_values(&self) -> &HashMap<String, Value> {
        self.default_values_cache.get_or_init(|| {
            let mut values = HashMap::new();
            for (k, v) in &self.json_schema.properties {
                if let Some(default) = &v.default {
                    values.insert(k.clone(), default.clone());
                }
            }
            values
        })
    }

    // ref: rxdb/src/rx-schema.ts:76-85
    /// Hash of the (stringified) JSON schema, computed via the user-supplied hash function.
    /// Upstream caches on first call via `overwriteGetterForCaching`; we just compute on demand
    /// because hashing is expected to be rare and `HashFunction` is async.
    pub async fn hash(&self) -> String {
        let value = serde_json::to_value(&self.json_schema).unwrap_or(Value::Null);
        let sorted = sort_object(&value, true);
        let s = serde_json::to_string(&sorted).unwrap_or_default();
        self.hash_function.hash(s).await
    }

    // ref: rxdb/src/rx-schema.ts:87-104
    /// Checks if a given change on a document is allowed.
    /// Ensures that final fields are not modified.
    pub fn validate_change(&self, data_before: &Value, data_after: &Value) -> RxResult<()> {
        for field_name in &self.final_fields {
            let before = data_before.get(field_name).cloned().unwrap_or(Value::Null);
            let after = data_after.get(field_name).cloned().unwrap_or(Value::Null);
            if !deep_equal(&before, &after) {
                return Err(new_rx_error(
                    "DOC9",
                    Some(json!({
                        "dataBefore": data_before,
                        "dataAfter": data_after,
                        "fieldName": field_name,
                        "schema": serde_json::to_value(&self.json_schema).unwrap_or(Value::Null),
                    })),
                ));
            }
        }
        Ok(())
    }

    // ref: rxdb/src/rx-schema.ts:106-174
    // `getDocumentPrototype()` — JS-specific prototype construction.
    // T1 deviation: omitted (no prototype chain in Rust; user code accesses
    // documents via `serde_json::Value` or per-collection deserialization).

    // ref: rxdb/src/rx-schema.ts:177-184
    pub fn get_primary_of_document_data(&self, document_data: &Value) -> RxResult<String> {
        get_composed_primary_key_of_document_data(&self.json_schema, document_data)
    }
}

// ref: rxdb/src/rx-schema.ts:187-191
pub fn get_indexes(json_schema: &RxJsonSchema) -> Vec<Vec<String>> {
    json_schema.indexes.clone()
}

// ref: rxdb/src/rx-schema.ts:193-202
/// array with previous version-numbers
pub fn get_previous_versions(schema: &RxJsonSchema) -> Vec<i32> {
    let version = schema.version.max(0);
    (0..version).collect()
}

// ref: rxdb/src/rx-schema.ts:204-220
pub fn create_rx_schema(
    json_schema: RxJsonSchema,
    hash_function: SharedHashFunction,
    run_pre_create_hooks: bool,
) -> RxResult<RxSchema> {
    let original_value = serde_json::to_value(&json_schema).unwrap_or(Value::Null);
    let mut as_value = original_value.clone();
    if run_pre_create_hooks {
        run_plugin_hooks("preCreateRxSchema", &mut as_value);
    }
    // hooks might have mutated the schema in-place; deserialize back.
    let mutated: RxJsonSchema = if as_value == original_value {
        json_schema
    } else {
        serde_json::from_value(as_value).unwrap_or(json_schema)
    };
    let filled = fill_with_default_settings(mutated);
    let normalized = normalize_rx_json_schema(&filled);
    let _ = (OVERWRITABLE.load().deep_freeze_when_dev_mode)(clone_deep(
        &serde_json::to_value(&normalized).unwrap_or(Value::Null),
    ));
    let schema = RxSchema::new(normalized, hash_function)?;
    let mut payload = serde_json::to_value(&schema.json_schema).unwrap_or(Value::Null);
    run_plugin_hooks("createRxSchema", &mut payload);
    Ok(schema)
}

// ref: rxdb/src/rx-schema.ts:222-224
/// True if the value can be deserialized as an `RxJsonSchema`. Replaces
/// upstream `instanceof RxSchema` since Rust does not have prototype checks.
pub fn is_rx_schema(value: &Value) -> bool {
    serde_json::from_value::<RxJsonSchema>(value.clone()).is_ok()
}

// ref: rxdb/src/rx-schema.ts:226-232
/// Identity helper that exists only for TypeScript type narrowing upstream.
/// In Rust the function is unnecessary; preserved as a no-op for API parity.
pub fn to_typed_rx_json_schema(schema: RxJsonSchema) -> RxJsonSchema {
    schema
}
