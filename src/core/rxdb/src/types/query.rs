//! Mango query and query-plan types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ref: rxdb/src/types/rx-storage.d.ts MangoQuerySelector<RxDocType>
pub type MangoQuerySelector = Value;

// ref: rxdb/src/types/rx-query.d.ts MangoQuery<RxDocType>
//
// User-facing query type. `selector`, `sort` and `index` are optional;
// `normalize_mango_query` fills them into a `FilledMangoQuery`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct MangoQuery {
    #[serde(default)]
    pub selector: Option<Value>,
    /// Each sort entry is `{fieldName: "asc"|"desc"}`.
    #[serde(default)]
    pub sort: Option<Vec<HashMap<String, String>>>,
    #[serde(default)]
    pub index: Option<Vec<String>>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub skip: Option<u64>,
}

// ref: rxdb/src/types/rx-query.d.ts FilledMangoQuery<RxDocType>
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct FilledMangoQuery {
    pub selector: Value,
    /// Each sort entry is `{fieldName: "asc"|"desc"}`. Always non-empty after
    /// `normalize_mango_query`.
    #[serde(default)]
    pub sort: Vec<HashMap<String, String>>,
    #[serde(default)]
    pub index: Option<Vec<String>>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub skip: Option<u64>,
}

// ref: rxdb/src/types/rx-query.d.ts RxQueryPlanKey
pub type RxQueryPlanKey = Value;

// ref: rxdb/src/types/rx-query.d.ts RxQueryPlanerOpts
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxQueryPlanerOpts {
    #[serde(rename = "startKey")]
    pub start_key: Value,
    #[serde(rename = "endKey")]
    pub end_key: Value,
    #[serde(rename = "inclusiveStart")]
    pub inclusive_start: bool,
    #[serde(rename = "inclusiveEnd")]
    pub inclusive_end: bool,
}

#[derive(Default, Debug, Clone)]
pub struct PartialRxQueryPlanerOpts {
    pub start_key: Option<Value>,
    pub end_key: Option<Value>,
    pub inclusive_start: Option<bool>,
    pub inclusive_end: Option<bool>,
}

// ref: rxdb/src/types/rx-query.d.ts RxQueryPlan
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxQueryPlan {
    pub index: Vec<String>,
    #[serde(rename = "startKeys")]
    pub start_keys: Vec<Value>,
    #[serde(rename = "endKeys")]
    pub end_keys: Vec<Value>,
    #[serde(rename = "inclusiveStart")]
    pub inclusive_start: bool,
    #[serde(rename = "inclusiveEnd")]
    pub inclusive_end: bool,
    #[serde(rename = "sortSatisfiedByIndex")]
    pub sort_satisfied_by_index: bool,
    #[serde(rename = "selectorSatisfiedByIndex")]
    pub selector_satisfied_by_index: bool,
}
