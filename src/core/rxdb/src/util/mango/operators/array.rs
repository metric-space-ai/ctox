//! Port of `mingo/src/operators/query/array/`.

use serde_json::Value;

use crate::util::mango::core::{Options, QueryPredicate};
use crate::util::mango::operators::predicates::{
    create_query_operator, pred_elem_match, pred_size,
};

// ref: mingo/src/operators/query/array/elemMatch.ts:1-12
pub fn op_elem_match(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_elem_match)(selector, value, options)
}

// ref: mingo/src/operators/query/array/size.ts:1-9
pub fn op_size(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_size)(selector, value, options)
}
