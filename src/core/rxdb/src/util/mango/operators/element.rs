//! Port of `mingo/src/operators/query/element/`.

use std::sync::Arc;

use serde_json::Value;

use crate::util::mango::core::{Options, QueryPredicate};
use crate::util::mango::operators::predicates::{
    create_query_operator, is_array, pred_type, resolve,
};

// ref: mingo/src/operators/query/element/exists.ts:10-25
pub fn op_exists(selector: &str, value: &Value, _options: &Options) -> QueryPredicate {
    let nested = selector.contains('.');
    let b = match value {
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_f64().map(|x| x != 0.0).unwrap_or(false),
        Value::Null => false,
        _ => true,
    };
    let selector_owned = selector.to_string();

    // top-level keys and array elements (selector ends with `.<digits>`)
    let trailing_index = selector.rsplit_once('.').map_or(false, |(_, last)| {
        !last.is_empty() && last.chars().all(|c| c.is_ascii_digit())
    });

    if !nested || trailing_index {
        return Arc::new(move |o: &Value| (resolve(o, &selector_owned, false).is_some()) == b);
    }

    // For nested keys, mirror upstream's `resolveGraph` shortcut: we approximate
    // it by resolving the parent path and checking whether the leaf exists on
    // each candidate. Upstream traverses arrays element-wise; the unwrapArray
    // = false path of `resolve` already returns an array of resolved subgraphs,
    // so we just inspect that.
    let parent = selector
        .rsplit_once('.')
        .map(|(p, _)| p.to_string())
        .unwrap_or_default();
    let leaf = selector
        .rsplit_once('.')
        .map(|(_, l)| l.to_string())
        .unwrap_or_else(|| selector.to_string());

    Arc::new(move |o: &Value| {
        let parent_val = resolve(o, &parent, false);
        let leaf_check = |v: &Value| -> bool {
            match v {
                Value::Object(m) => m.contains_key(&leaf),
                _ => false,
            }
        };
        let exists = match parent_val {
            Some(ref v) if is_array(v) => {
                if let Value::Array(arr) = v {
                    arr.iter().any(leaf_check)
                } else {
                    false
                }
            }
            Some(ref v) => leaf_check(v),
            None => false,
        };
        exists == b
    })
}

// ref: mingo/src/operators/query/element/type.ts:1-9
pub fn op_type(selector: &str, value: &Value, options: &Options) -> QueryPredicate {
    create_query_operator(pred_type)(selector, value, options)
}
