//! Port of `mingo/src/operators/pipeline/{sort,project}.ts`.
//!
//! Both pipeline operators consume the full collection (mingo upstream uses a
//! lazy `Iterator`; we operate on `Vec<Value>` since the only call site in
//! CTOX materialises results immediately).

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::util::mango::core::Options;
use crate::util::mango::operators::predicates::{compare, resolve};

// ref: mingo/src/operators/pipeline/sort.ts:24-58
pub fn op_sort(collection: Vec<Value>, expr: &Value, _options: &Options) -> Vec<Value> {
    let sort_keys = match expr {
        Value::Object(m) if !m.is_empty() => m,
        _ => return collection,
    };

    let mut coll = collection;
    let modifiers: Vec<(&String, &Value)> = sort_keys.iter().collect();

    // upstream: `for (const key of modifiers.reverse())` — stable group sort
    // applied right-to-left so earlier keys take precedence.
    for (key, dir_val) in modifiers.into_iter().rev() {
        let descending = dir_val.as_i64().map(|n| n == -1).unwrap_or(false);

        // groupBy via insertion-ordered Map keyed by the JSON-string form of
        // the resolved key. Using `stringify` for the group key gives us
        // upstream's value-equality semantics without needing a real
        // `ValueMap`.
        let mut order: Vec<String> = Vec::new();
        let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
        let mut key_to_value: HashMap<String, Value> = HashMap::new();

        for obj in coll.drain(..) {
            let k = resolve(&obj, key, false).unwrap_or(Value::Null);
            let kstr = stable_stringify(&k);
            if !groups.contains_key(&kstr) {
                order.push(kstr.clone());
                key_to_value.insert(kstr.clone(), k);
                groups.insert(kstr.clone(), Vec::new());
            }
            groups.get_mut(&kstr).unwrap().push(obj);
        }

        order.sort_by(|a, b| {
            let av = key_to_value.get(a).unwrap();
            let bv = key_to_value.get(b).unwrap();
            compare(av, bv)
        });
        if descending {
            order.reverse();
        }

        for k in order {
            if let Some(items) = groups.remove(&k) {
                for v in items {
                    coll.push(v);
                }
            }
        }
    }
    coll
}

/// Stable, order-independent string form used as a HashMap key for grouping.
/// Mirrors the role of mingo's `stringify` (sorted keys) but only for the
/// hashmap key — values themselves keep their original ordering.
fn stable_stringify(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => format!("b:{b}"),
        Value::Number(n) => format!("n:{}", n),
        Value::String(s) => format!("s:{}", serde_json::to_string(s).unwrap_or_default()),
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(stable_stringify).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", k, stable_stringify(&m[k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

// ref: mingo/src/operators/pipeline/project.ts:41-49
pub fn op_project(collection: Vec<Value>, expr: &Value, options: &Options) -> Vec<Value> {
    let expr_map = match expr {
        Value::Object(m) if !m.is_empty() => m,
        _ => return collection,
    };
    validate_expression(expr_map, options);
    let id_key = options.id_key.clone();
    let expr_owned = expr_map.clone();
    collection
        .into_iter()
        .map(|o| project_one(&o, &expr_owned, &id_key))
        .collect()
}

// ref: mingo/src/operators/pipeline/project.ts:60-196
fn project_one(o: &Value, expr: &Map<String, Value>, id_key: &str) -> Value {
    // Classify keys in upstream order.
    let mut excluded_keys: Vec<&String> = Vec::new();
    let mut included_keys: Vec<&String> = Vec::new();
    let mut computed: Vec<(&String, &Value)> = Vec::new();

    for (k, sub) in expr.iter() {
        match sub {
            Value::Bool(true) | Value::Number(_) if is_truthy_inclusion(sub) => {
                included_keys.push(k);
            }
            Value::Bool(false) => excluded_keys.push(k),
            Value::Number(n) if n.as_f64().map(|x| x == 0.0).unwrap_or(false) => {
                excluded_keys.push(k);
            }
            _ => computed.push((k, sub)),
        }
    }

    let id_key_excluded = excluded_keys.iter().any(|k| k.as_str() == id_key);
    let id_key_only_excluded = id_key_excluded
        && excluded_keys.len() == 1
        && included_keys.is_empty()
        && computed.is_empty();

    if id_key_only_excluded {
        if let Value::Object(m) = o {
            let mut new_obj = m.clone();
            new_obj.shift_remove(id_key);
            return Value::Object(new_obj);
        }
        return o.clone();
    }

    let id_key_implicit = !id_key_excluded && !included_keys.iter().any(|k| k.as_str() == id_key);

    let mut new_obj = Map::new();

    if !excluded_keys.is_empty() && included_keys.is_empty() {
        if let Value::Object(m) = o {
            new_obj = m.clone();
            for k in &excluded_keys {
                new_obj.shift_remove(k.as_str());
            }
        }
    }

    for k in &included_keys {
        if let Some(v) = resolve(o, k, false) {
            new_obj.insert((*k).clone(), v);
        }
    }

    // computed sub-expressions: upstream supports projection operators,
    // pipeline operators, nested projections, and string `$path` references.
    // For the rx-query-mingo subset we only need literal values and direct
    // field references, since upstream never registers expression/projection
    // operators on the pipeline registry inside rx-query-mingo.
    for (k, sub) in &computed {
        let value = compute_simple(o, sub);
        if let Some(v) = value {
            new_obj.insert((*k).clone(), v);
        } else {
            new_obj.shift_remove(k.as_str());
        }
    }

    if id_key_implicit {
        if let Some(v) = resolve(o, id_key, false) {
            new_obj.insert(id_key.to_string(), v);
        }
    }
    Value::Object(new_obj)
}

fn is_truthy_inclusion(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|x| x != 0.0).unwrap_or(false),
        _ => false,
    }
}

fn compute_simple(o: &Value, sub: &Value) -> Option<Value> {
    if let Value::String(s) = sub {
        if let Some(stripped) = s.strip_prefix('$') {
            return resolve(o, stripped, false);
        }
    }
    Some(sub.clone())
}

// ref: mingo/src/operators/pipeline/project.ts:198-218
fn validate_expression(expr: &Map<String, Value>, options: &Options) {
    let mut exclusions = false;
    let mut inclusions = false;
    for (k, v) in expr.iter() {
        debug_assert!(!k.starts_with('$'), "Field names may not start with '$'.");
        debug_assert!(
            !k.ends_with(".$"),
            "Positional projection operator '$' is not supported."
        );
        if k == &options.id_key {
            continue;
        }
        match v {
            Value::Number(n) if n.as_f64().map(|x| x == 0.0).unwrap_or(false) => exclusions = true,
            Value::Bool(false) => exclusions = true,
            Value::Number(n) if n.as_f64().map(|x| x == 1.0).unwrap_or(false) => inclusions = true,
            Value::Bool(true) => inclusions = true,
            _ => {}
        }
        debug_assert!(
            !(exclusions && inclusions),
            "Projection cannot have a mix of inclusion and exclusion."
        );
    }
}
