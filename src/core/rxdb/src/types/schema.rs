//! Schema types — port of the schema-related entries in `rxdb/src/types/index.d.ts`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

// ref: rxdb/src/types/rx-schema.d.ts CompositePrimaryKey<RxDocType>
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompositePrimaryKey {
    pub key: String,
    pub fields: Vec<String>,
    pub separator: String,
}

// ref: rxdb/src/types/rx-schema.d.ts PrimaryKey<RxDocType>
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum PrimaryKey {
    /// Simple form: the name of a top-level property.
    Simple(String),
    /// Composite form: derived from multiple fields joined by `separator`.
    Composite(CompositePrimaryKey),
}

impl PrimaryKey {
    /// Returns the *storage* primary field name. For [`PrimaryKey::Simple`]
    /// this is the property; for [`PrimaryKey::Composite`] it is `.key`.
    pub fn primary_field(&self) -> &str {
        match self {
            Self::Simple(s) => s,
            Self::Composite(c) => &c.key,
        }
    }
}

/// JSON Schema permits either one type or a union of types. Keep the original
/// representation when serializing: schema hashes must not discard nullability.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum JsonSchemaType {
    Single(String),
    Multiple(Vec<String>),
}

impl From<String> for JsonSchemaType {
    fn from(value: String) -> Self {
        Self::Single(value)
    }
}

impl From<&str> for JsonSchemaType {
    fn from(value: &str) -> Self {
        Self::Single(value.to_owned())
    }
}

impl JsonSchemaType {
    /// Only a single declared type is safe for homogeneous index optimizations.
    pub fn single_type(&self) -> Option<&str> {
        match self {
            Self::Single(value) => Some(value),
            Self::Multiple(_) => None,
        }
    }

    pub fn includes(&self, name: &str) -> bool {
        match self {
            Self::Single(value) => value == name,
            Self::Multiple(values) => values.iter().any(|value| value == name),
        }
    }

    /// Projection defaults prefer explicit nullability, otherwise the first
    /// declared type. This does not change the serialized union ordering.
    pub fn default_type(&self) -> Option<&str> {
        match self {
            Self::Single(value) => Some(value),
            Self::Multiple(values) => values
                .iter()
                .find(|value| value.as_str() == "null")
                .or_else(|| values.first())
                .map(String::as_str),
        }
    }

    pub fn matches_value(&self, value: &Value) -> bool {
        (value.is_null() && self.includes("null"))
            || (value.is_string() && self.includes("string"))
            || (value.is_number() && self.includes("number"))
            || ((value.is_i64() || value.is_u64()) && self.includes("integer"))
            || (value.is_boolean() && self.includes("boolean"))
            || (value.is_object() && self.includes("object"))
            || (value.is_array() && self.includes("array"))
    }
}

// ref: rxdb/src/types/rx-schema.d.ts JsonSchema<RxDocType>
//
// JSON-Schema fragment. Upstream is a TS conditional/recursive type. Here we
// expose the fields actually read/written by ported code, and keep an `extra`
// catch-all for anything upstream emits that we don't model yet.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JsonSchema {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<JsonSchemaType>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, JsonSchema>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,

    #[serde(rename = "maxLength", default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,
    #[serde(rename = "minLength", default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_optional_json_number"
    )]
    pub minimum: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_optional_json_number"
    )]
    pub maximum: Option<f64>,
    #[serde(
        rename = "multipleOf",
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_optional_json_number"
    )]
    pub multiple_of: Option<f64>,

    #[serde(
        rename = "additionalProperties",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_properties: Option<bool>,

    #[serde(rename = "final", default, skip_serializing_if = "Option::is_none")]
    pub final_field: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,

    /// Anything the upstream emits that this struct does not model yet.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

// ref: rxdb/src/types/rx-schema.d.ts:148-154
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RxJsonSchemaAttachments {
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
}

// ref: rxdb/src/types/rx-schema.d.ts RxJsonSchema<RxDocType>
//
// The top-level schema. T1 decision: keep this concrete (not generic over
// document type) because rxdb-rs documents are dynamically typed via
// `serde_json::Value`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxJsonSchema {
    #[serde(default)]
    pub version: i32,

    #[serde(rename = "primaryKey")]
    pub primary_key: PrimaryKey,

    #[serde(rename = "type", default = "default_object_type")]
    pub schema_type: String,

    pub properties: HashMap<String, JsonSchema>,

    #[serde(default)]
    pub required: Vec<String>,

    #[serde(default)]
    pub indexes: Vec<Vec<String>>,

    #[serde(default)]
    pub encrypted: Vec<String>,

    #[serde(
        rename = "internalIndexes",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub internal_indexes: Vec<Vec<String>>,

    #[serde(rename = "keyCompression", default)]
    pub key_compression: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<RxJsonSchemaAttachments>,

    #[serde(rename = "additionalProperties", default)]
    pub additional_properties: bool,

    /// Forward-compat catch-all for unmodelled top-level fields.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

fn default_object_type() -> String {
    "object".to_string()
}

fn serialize_optional_json_number<S>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(number) if number.is_finite() && number.fract() == 0.0 => {
            serializer.serialize_i64(*number as i64)
        }
        Some(number) => serializer.serialize_f64(*number),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod schema_type_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn union_types_round_trip_without_losing_nullability() {
        for declared in [
            json!("string"),
            json!(["string", "null"]),
            json!(["integer", "boolean"]),
        ] {
            let fragment: JsonSchema = serde_json::from_value(json!({"type": declared})).unwrap();
            assert_eq!(serde_json::to_value(&fragment).unwrap()["type"], declared);
        }
        let nullable: JsonSchema =
            serde_json::from_value(json!({"type": ["string", "null"]})).unwrap();
        let declared = nullable.schema_type.unwrap();
        assert!(declared.matches_value(&json!("task-a")));
        assert!(declared.matches_value(&Value::Null));
        assert!(!declared.matches_value(&json!(42)));
        assert!(!declared.matches_value(&json!(false)));
        assert!(!declared.matches_value(&json!({})));
        assert_eq!(declared.single_type(), None);
        assert_eq!(declared.default_type(), Some("null"));
        assert!(serde_json::from_value::<JsonSchema>(json!({"type": ["string", 7]})).is_err());
    }

    #[test]
    fn every_packaged_business_os_schema_deserializes_and_retains_types() {
        let contract: HashMap<String, Value> = serde_json::from_str(include_str!(
            "../../../business_os/business_os_schema_contract.json"
        ))
        .unwrap();
        assert!(!contract.is_empty());
        fn verify_types(expected: &Value, actual: &Value) {
            if let Some(declared) = expected.get("type") {
                assert_eq!(actual.get("type"), Some(declared));
            }
            if let Some(properties) = expected.get("properties").and_then(Value::as_object) {
                for (name, property) in properties {
                    verify_types(property, &actual["properties"][name]);
                }
            }
            if let Some(items) = expected.get("items") {
                verify_types(items, &actual["items"]);
            }
        }
        for (name, mut original) in contract {
            // The native Business OS entry point supports both index notations.
            if let Some(indexes) = original.get_mut("indexes").and_then(Value::as_array_mut) {
                for index in indexes {
                    if index.is_string() {
                        *index = json!([index.clone()]);
                    }
                }
            }
            let parsed: RxJsonSchema = serde_json::from_value(original.clone())
                .unwrap_or_else(|error| panic!("packaged schema {name}: {error}"));
            verify_types(&original, &serde_json::to_value(parsed).unwrap());
        }
    }
}
