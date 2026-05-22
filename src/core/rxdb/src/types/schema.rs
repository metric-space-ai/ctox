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

// ref: rxdb/src/types/rx-schema.d.ts JsonSchema<RxDocType>
//
// JSON-Schema fragment. Upstream is a TS conditional/recursive type. Here we
// expose the fields actually read/written by ported code, and keep an `extra`
// catch-all for anything upstream emits that we don't model yet.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JsonSchema {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,

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
