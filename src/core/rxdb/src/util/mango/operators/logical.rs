//! Port of `mingo/src/operators/query/logical/`.
//!
//! Each logical operator constructs sub-`Query` instances from its array of
//! clauses and combines their `test` results.

use std::sync::Arc;

use serde_json::Value;

use crate::util::mango::core::{Options, QueryPredicate};
use crate::util::mango::operators::predicates::normalize;
use crate::util::mango::query::Query;

// ref: mingo/src/operators/query/logical/and.ts:15-26
pub fn op_and(_selector: &str, rhs: &Value, _options: &Options) -> QueryPredicate {
    // upstream: assert(isArray(rhs), "Invalid expression: $and expects value to be an Array.")
    let queries: Vec<Query> = match rhs {
        Value::Array(arr) => arr.iter().map(Query::new).collect(),
        _ => Vec::new(),
    };
    Arc::new(move |obj: &Value| queries.iter().all(|q| q.test(obj)))
}

// ref: mingo/src/operators/query/logical/or.ts:15-23
pub fn op_or(_selector: &str, rhs: &Value, _options: &Options) -> QueryPredicate {
    let queries: Vec<Query> = match rhs {
        Value::Array(arr) => arr.iter().map(Query::new).collect(),
        _ => Vec::new(),
    };
    // upstream falls through to `.some(q => q.test(obj))`; an empty array
    // therefore matches nothing, so a malformed `$or: <non-array>` returns
    // false here as well — matching upstream after its `assert` panics in JS.
    Arc::new(move |obj: &Value| queries.iter().any(|q| q.test(obj)))
}

// ref: mingo/src/operators/query/logical/nor.ts:15-26
pub fn op_nor(_selector: &str, rhs: &Value, options: &Options) -> QueryPredicate {
    let inner = op_or("$or", rhs, options);
    Arc::new(move |obj: &Value| !(inner)(obj))
}

// ref: mingo/src/operators/query/logical/not.ts:15-24
pub fn op_not(selector: &str, rhs: &Value, _options: &Options) -> QueryPredicate {
    // const criteria = {}; criteria[selector] = normalize(rhs);
    let mut criteria = serde_json::Map::new();
    criteria.insert(selector.to_string(), normalize(rhs));
    let q = Query::new(&Value::Object(criteria));
    Arc::new(move |obj: &Value| !q.test(obj))
}
