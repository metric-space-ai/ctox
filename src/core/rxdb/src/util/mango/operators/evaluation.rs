//! Port of `mingo/src/operators/query/evaluation/`.

use serde_json::Value;

use crate::util::mango::core::{Options, QueryPredicate};
use crate::util::mango::operators::predicates::{create_query_operator, pred_mod, pred_regex};

// ref: mingo/src/operators/query/evaluation/regex.ts:1-9
pub fn op_regex(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_regex)(selector, value, options)
}

// ref: mingo/src/operators/query/evaluation/mod.ts:1-9
pub fn op_mod(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_mod)(selector, value, options)
}
