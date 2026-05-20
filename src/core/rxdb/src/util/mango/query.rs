//! Port of `mingo/src/query.ts` together with the rx-query-mingo entry point
//! (`getMingoQuery`).
//!
//! Compiles a mango selector into a list of predicates and exposes `test` /
//! `find` against `serde_json::Value` documents.

use std::sync::Arc;

use serde_json::Value;

use crate::util::mango::core::{
    get_operator, use_operators_pipeline, use_operators_query, Context, Operator, OperatorType,
    Options, QueryPredicate,
};
use crate::util::mango::operators::predicates::{is_operator, normalize};
use crate::util::mango::operators::{array, comparison, element, evaluation, logical, pipeline};

/// Type alias matching the public surface promised by the task brief. Mango
/// selectors are JSON objects.
pub type MangoSelector = Value;

// ref: mingo/src/query.ts:7-9
const TOP_LEVEL_OPS: &[&str] = &["$and", "$or", "$nor"];

// ref: mingo/src/query.ts:18-111
/// An object used to filter input documents.
#[derive(Clone)]
pub struct Query {
    compiled: Vec<QueryPredicate>,
    options: Options,
    condition: Value,
}

impl std::fmt::Debug for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field("condition", &self.condition)
            .field("compiled_len", &self.compiled.len())
            .finish()
    }
}

impl Query {
    // ref: mingo/src/query.ts:23-28
    pub fn new(condition: &Value) -> Self {
        let mut ctx = Context::init();
        register_default_ops(&mut ctx);
        let options = Options::init(ctx);
        let mut q = Self {
            compiled: Vec::new(),
            options,
            // upstream: cloneDeep(condition)
            condition: condition.clone(),
        };
        q.compile();
        q
    }

    // ref: mingo/src/query.ts:30-65
    fn compile(&mut self) {
        // upstream asserts `isObject(condition)`. JSON null/array selectors
        // are programming errors at the call site; we treat them as empty
        // condition (matching everything) rather than panicking.
        let map = match &self.condition {
            Value::Object(m) => m.clone(),
            _ => return,
        };

        for (field, expr) in map.iter() {
            if field == "$where" {
                // scriptEnabled is true by default but `$where` is explicitly
                // out of scope per the brief. Skip it.
                continue;
            } else if TOP_LEVEL_OPS.contains(&field.as_str()) {
                self.process_operator(field, field, expr);
            } else {
                // upstream: assert(!isOperator(field), "unknown top level operator: ...")
                debug_assert!(!is_operator(field), "unknown top level operator: {}", field);
                let normalized = normalize(expr);
                if let Value::Object(sub) = &normalized {
                    for (operator, val) in sub.iter() {
                        self.process_operator(field, operator, val);
                    }
                }
            }
        }
    }

    // ref: mingo/src/query.ts:67-71
    fn process_operator(&mut self, field: &str, operator: &str, value: &Value) {
        let op = get_operator(&self.options.context, OperatorType::Query, operator);
        match op {
            Some(Operator::Query(call)) => {
                let pred = call(field, value, &self.options);
                self.compiled.push(pred);
            }
            // upstream asserts an unknown operator. We push an always-false
            // predicate so the surrounding query keeps composing, then
            // surface the issue through a debug_assert for tests.
            _ => {
                debug_assert!(false, "unknown query operator {}", operator);
                let op_name = operator.to_string();
                self.compiled.push(Arc::new(move |_: &Value| {
                    debug_assert!(false, "unknown query operator {}", op_name);
                    false
                }));
            }
        }
    }

    // ref: mingo/src/query.ts:79-81
    pub fn test(&self, obj: &Value) -> bool {
        self.compiled.iter().all(|p| p(obj))
    }

    // ref: mingo/src/query.ts:90-97
    /// Returns the documents from `collection` that match this query.
    ///
    /// Upstream returns a lazy `Cursor`; we return owned references which the
    /// caller can clone or further process.
    pub fn find<'a>(&self, collection: &'a [Value]) -> Vec<&'a Value> {
        collection.iter().filter(|o| self.test(o)).collect()
    }

    // ref: mingo/src/query.ts:105-110
    /// Remove matched documents from the collection, returning the remainder.
    #[allow(dead_code)]
    pub fn remove<'a>(&self, collection: &'a [Value]) -> Vec<&'a Value> {
        collection.iter().filter(|o| !self.test(o)).collect()
    }
}

// ref: rxdb/src/rx-query-mingo.ts:47-78
/// Build a query with the same operator wiring as
/// `vendor/rxdb-16.20.0/src/rx-query-mingo.ts`.
pub fn get_mingo_query(selector: &Value) -> Query {
    // The operator registration is per-Query — there is no global init flag
    // because `Query::new` already populates the context via
    // `register_default_ops`. The `mingoInitDone` flag upstream is an artifact
    // of mingo's mutable global registry.
    Query::new(selector)
}

// ref: rxdb/src/rx-query-mingo.ts:50-75
fn register_default_ops(ctx: &mut Context) {
    use_operators_pipeline(
        ctx,
        vec![
            ("$sort", pipeline::op_sort as _),
            ("$project", pipeline::op_project as _),
        ],
    );
    use_operators_query(
        ctx,
        vec![
            ("$and", logical::op_and as _),
            ("$eq", comparison::op_eq as _),
            ("$elemMatch", array::op_elem_match as _),
            ("$exists", element::op_exists as _),
            ("$gt", comparison::op_gt as _),
            ("$gte", comparison::op_gte as _),
            ("$in", comparison::op_in as _),
            ("$lt", comparison::op_lt as _),
            ("$lte", comparison::op_lte as _),
            ("$ne", comparison::op_ne as _),
            ("$nin", comparison::op_nin as _),
            ("$mod", evaluation::op_mod as _),
            ("$nor", logical::op_nor as _),
            ("$not", logical::op_not as _),
            ("$or", logical::op_or as _),
            ("$regex", evaluation::op_regex as _),
            ("$size", array::op_size as _),
            ("$type", element::op_type as _),
        ],
    );
}

/// Convenience: run the `$sort` pipeline operator against a vector of docs.
/// Future ports of `rx-query-mingo.ts` (and `rx-query.ts` consumers) call this
/// to materialise sorted results from a `MangoQuerySortPart`.
// ref: mingo/src/operators/pipeline/sort.ts:24-58
pub fn sort_documents(docs: &[Value], sort_spec: &Value) -> Vec<Value> {
    let mut ctx = Context::init();
    register_default_ops(&mut ctx);
    let opts = Options::init(ctx);
    pipeline::op_sort(docs.to_vec(), sort_spec, &opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(json: serde_json::Value) -> Value {
        json
    }

    // ---------- comparison ----------

    #[test]
    fn eq_matches_simple_field() {
        let q = get_mingo_query(&json!({ "a": 1 }));
        assert!(q.test(&doc(json!({ "a": 1 }))));
        assert!(!q.test(&doc(json!({ "a": 2 }))));
    }

    #[test]
    fn ne_matches_inequality() {
        let q = get_mingo_query(&json!({ "a": { "$ne": 1 } }));
        assert!(!q.test(&doc(json!({ "a": 1 }))));
        assert!(q.test(&doc(json!({ "a": 2 }))));
    }

    #[test]
    fn gt_lt_gte_lte_numbers() {
        let docs = vec![json!({ "n": 1 }), json!({ "n": 5 }), json!({ "n": 10 })];
        let gt = get_mingo_query(&json!({ "n": { "$gt": 5 } }));
        assert_eq!(gt.find(&docs).len(), 1);

        let gte = get_mingo_query(&json!({ "n": { "$gte": 5 } }));
        assert_eq!(gte.find(&docs).len(), 2);

        let lt = get_mingo_query(&json!({ "n": { "$lt": 5 } }));
        assert_eq!(lt.find(&docs).len(), 1);

        let lte = get_mingo_query(&json!({ "n": { "$lte": 5 } }));
        assert_eq!(lte.find(&docs).len(), 2);
    }

    #[test]
    fn in_and_nin() {
        let q_in = get_mingo_query(&json!({ "a": { "$in": [1, 2, 3] } }));
        assert!(q_in.test(&doc(json!({ "a": 2 }))));
        assert!(!q_in.test(&doc(json!({ "a": 4 }))));

        let q_nin = get_mingo_query(&json!({ "a": { "$nin": [1, 2, 3] } }));
        assert!(!q_nin.test(&doc(json!({ "a": 2 }))));
        assert!(q_nin.test(&doc(json!({ "a": 4 }))));
    }

    // ---------- logical ----------

    #[test]
    fn and_or_nor_not() {
        let docs = vec![
            json!({ "n": 1, "s": "x" }),
            json!({ "n": 2, "s": "y" }),
            json!({ "n": 3, "s": "z" }),
        ];
        let q_and = get_mingo_query(&json!({
            "$and": [ { "n": { "$gte": 2 } }, { "s": "y" } ]
        }));
        assert_eq!(q_and.find(&docs).len(), 1);

        let q_or = get_mingo_query(&json!({
            "$or": [ { "n": 1 }, { "s": "z" } ]
        }));
        assert_eq!(q_or.find(&docs).len(), 2);

        let q_nor = get_mingo_query(&json!({
            "$nor": [ { "n": 1 }, { "s": "z" } ]
        }));
        assert_eq!(q_nor.find(&docs).len(), 1);

        let q_not = get_mingo_query(&json!({ "n": { "$not": { "$gt": 1 } } }));
        assert!(q_not.test(&doc(json!({ "n": 1 }))));
        assert!(!q_not.test(&doc(json!({ "n": 2 }))));
    }

    // ---------- array ----------

    #[test]
    fn elem_match_and_size() {
        let q_elem = get_mingo_query(&json!({
            "tags": { "$elemMatch": { "$gt": 2 } }
        }));
        assert!(q_elem.test(&doc(json!({ "tags": [1, 2, 3] }))));
        assert!(!q_elem.test(&doc(json!({ "tags": [1, 2] }))));

        let q_size = get_mingo_query(&json!({ "tags": { "$size": 3 } }));
        assert!(q_size.test(&doc(json!({ "tags": [1, 2, 3] }))));
        assert!(!q_size.test(&doc(json!({ "tags": [1, 2] }))));
    }

    // ---------- element ----------

    #[test]
    fn exists_and_type() {
        let q_exists = get_mingo_query(&json!({ "a": { "$exists": true } }));
        assert!(q_exists.test(&doc(json!({ "a": 1 }))));
        assert!(!q_exists.test(&doc(json!({ "b": 1 }))));

        let q_type_str = get_mingo_query(&json!({ "s": { "$type": "string" } }));
        assert!(q_type_str.test(&doc(json!({ "s": "hi" }))));
        assert!(!q_type_str.test(&doc(json!({ "s": 1 }))));

        let q_type_num = get_mingo_query(&json!({ "n": { "$type": "number" } }));
        assert!(q_type_num.test(&doc(json!({ "n": 5 }))));
        assert!(!q_type_num.test(&doc(json!({ "n": "x" }))));
    }

    // ---------- evaluation ----------

    #[test]
    fn regex_and_mod() {
        let q_regex = get_mingo_query(&json!({ "s": { "$regex": "^foo" } }));
        assert!(q_regex.test(&doc(json!({ "s": "foobar" }))));
        assert!(!q_regex.test(&doc(json!({ "s": "barfoo" }))));

        let q_mod = get_mingo_query(&json!({ "n": { "$mod": [2, 0] } }));
        assert!(q_mod.test(&doc(json!({ "n": 4 }))));
        assert!(!q_mod.test(&doc(json!({ "n": 5 }))));
    }

    // ---------- pipeline ----------

    #[test]
    fn sort_documents_ascending_and_descending() {
        let docs = vec![json!({ "n": 3 }), json!({ "n": 1 }), json!({ "n": 2 })];
        let asc = sort_documents(&docs, &json!({ "n": 1 }));
        assert_eq!(
            asc.iter()
                .map(|d| d["n"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let desc = sort_documents(&docs, &json!({ "n": -1 }));
        assert_eq!(
            desc.iter()
                .map(|d| d["n"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
    }

    #[test]
    fn find_filters_collection() {
        let docs = vec![
            json!({ "a": 1, "b": 2 }),
            json!({ "a": 1, "b": 3 }),
            json!({ "a": 2, "b": 2 }),
        ];
        let q = get_mingo_query(&json!({ "a": 1, "b": { "$gt": 2 } }));
        let res = q.find(&docs);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0]["b"].as_i64().unwrap(), 3);
    }
}
