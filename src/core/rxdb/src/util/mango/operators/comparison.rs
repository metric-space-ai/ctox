//! Port of `mingo/src/operators/query/comparison/`.
//!
//! Each operator is a one-line wrapper around the predicate from
//! `predicates.rs`, mirroring upstream where each comparison file is just:
//! ```ts
//! export const $eq = createQueryOperator(__eq);
//! ```

use serde_json::Value;

use crate::util::mango::core::{Options, QueryPredicate};
use crate::util::mango::operators::predicates::{
    create_query_operator, pred_eq, pred_gt, pred_gte, pred_in, pred_lt, pred_lte, pred_ne,
    pred_nin,
};

// ref: mingo/src/operators/query/comparison/eq.ts:1-8
pub fn op_eq(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_eq)(selector, value, options)
}

// ref: mingo/src/operators/query/comparison/ne.ts:1-8
pub fn op_ne(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_ne)(selector, value, options)
}

// ref: mingo/src/operators/query/comparison/gt.ts:1-8
pub fn op_gt(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_gt)(selector, value, options)
}

// ref: mingo/src/operators/query/comparison/gte.ts:1-8
pub fn op_gte(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_gte)(selector, value, options)
}

// ref: mingo/src/operators/query/comparison/lt.ts:1-8
pub fn op_lt(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_lt)(selector, value, options)
}

// ref: mingo/src/operators/query/comparison/lte.ts:1-8
pub fn op_lte(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_lte)(selector, value, options)
}

// ref: mingo/src/operators/query/comparison/in.ts:1-8
pub fn op_in(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_in)(selector, value, options)
}

// ref: mingo/src/operators/query/comparison/nin.ts:1-8
pub fn op_nin(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_nin)(selector, value, options)
}
