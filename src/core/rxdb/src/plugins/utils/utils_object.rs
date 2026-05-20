//! Object / JSON-value utilities.

use serde_json::{Map, Value};

use crate::rx_error::{new_rx_error, RxResult};

// ref: rxdb/src/plugins/utils/utils-object.ts:5-22
/// `deepFreeze` is a no-op in Rust: values are owned and immutable by default.
/// Provided for API parity.
pub fn deep_freeze<T>(o: T) -> T {
    o
}

pub type ObjectPathMonadFunction = Box<dyn Fn(&Value) -> Value + Send + Sync>;

// ref: rxdb/src/plugins/utils/utils-object.ts:34-61
/// To get specific nested path values from objects,
/// RxDB normally uses the 'dot-prop' npm module.
/// But when performance is really relevant, this is not fast enough.
/// Instead we use a monad that can prepare some stuff up front
/// and we can reuse the generated function.
pub fn object_path_monad(object_path: &str) -> ObjectPathMonadFunction {
    let split: Vec<String> = object_path.split('.').map(|s| s.to_string()).collect();
    let split_length = split.len();
    if split_length == 1 {
        let key = split.into_iter().next().unwrap();
        return Box::new(move |obj: &Value| obj.get(&key).cloned().unwrap_or(Value::Null));
    }
    Box::new(move |obj: &Value| {
        let mut current: &Value = obj;
        for sub in &split {
            match current.get(sub) {
                Some(v) => current = v,
                None => return Value::Null,
            }
        }
        current.clone()
    })
}

// ref: rxdb/src/plugins/utils/utils-object.ts:64-73
pub fn get_from_object_or_throw(obj: &Value, key: &str) -> RxResult<Value> {
    match obj.get(key) {
        Some(v) if !v.is_null() => Ok(v.clone()),
        _ => Err(new_rx_error(
            "UTL5",
            Some(serde_json::json!({ "message": format!("missing value from object {key}") })),
        )),
    }
}

// ref: rxdb/src/plugins/utils/utils-object.ts:78-95
/// returns a flattened object using dot-notation keys.
pub fn flatten_object(ob: &Value) -> Value {
    let mut out = Map::new();
    fn rec(prefix: &str, ob: &Value, out: &mut Map<String, Value>) {
        if let Value::Object(map) = ob {
            for (k, v) in map.iter() {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                if v.is_object() {
                    rec(&key, v, out);
                } else {
                    out.insert(key, v.clone());
                }
            }
        }
    }
    rec("", ob, &mut out);
    Value::Object(out)
}

// ref: rxdb/src/plugins/utils/utils-object.ts:102-105
/// does a flat copy. For `serde_json::Value` this is just `.clone()`.
pub fn flat_clone(obj: &Value) -> Value {
    obj.clone()
}

// ref: rxdb/src/plugins/utils/utils-object.ts:109-112
pub fn first_property_name_of_object(obj: &Value) -> Option<String> {
    obj.as_object().and_then(|m| m.keys().next().cloned())
}

// ref: rxdb/src/plugins/utils/utils-object.ts:113-116
pub fn first_property_value_of_object(obj: &Value) -> Option<Value> {
    obj.as_object().and_then(|m| m.values().next().cloned())
}

// ref: rxdb/src/plugins/utils/utils-object.ts:121-153
/// deep-sort an object so its attributes are in lexical order.
/// Also sorts the arrays inside of the object if `no_array_sort` is false.
pub fn sort_object(obj: &Value, no_array_sort: bool) -> Value {
    match obj {
        Value::Null => Value::Null,
        Value::Bool(_) | Value::Number(_) | Value::String(_) => obj.clone(),
        Value::Array(arr) => {
            if no_array_sort {
                Value::Array(arr.iter().map(|i| sort_object(i, no_array_sort)).collect())
            } else {
                let mut sorted: Vec<Value> = arr.clone();
                sorted.sort_by(|a, b| match (a, b) {
                    (Value::String(x), Value::String(y)) => x.cmp(y),
                    (Value::Object(_), _) => std::cmp::Ordering::Greater,
                    _ => std::cmp::Ordering::Less,
                });
                Value::Array(
                    sorted
                        .into_iter()
                        .map(|i| sort_object(&i, no_array_sort))
                        .collect(),
                )
            }
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                out.insert(k.clone(), sort_object(&map[k], no_array_sort));
            }
            Value::Object(out)
        }
    }
}

// ref: rxdb/src/plugins/utils/utils-object.ts:165-188
/// Deep clone a plain json value.
/// For `serde_json::Value` this is `.clone()`; provided for API parity.
pub fn clone_deep(src: &Value) -> Value {
    src.clone()
}

// ref: rxdb/src/plugins/utils/utils-object.ts:188
/// alias for `clone_deep`
pub fn clone(src: &Value) -> Value {
    clone_deep(src)
}

// ref: rxdb/src/plugins/utils/utils-object.ts:196-207
// `overwriteGetterForCaching` — JS-specific (Object.defineProperty getter).
// Rust has no analog; omitted.

// ref: rxdb/src/plugins/utils/utils-object.ts:210-231
pub fn has_deep_property(obj: &Value, property: &str) -> bool {
    match obj {
        Value::Object(map) => {
            if map.contains_key(property) {
                return true;
            }
            for v in map.values() {
                if v.is_object() || v.is_array() {
                    if has_deep_property(v, property) {
                        return true;
                    }
                }
            }
            false
        }
        Value::Array(arr) => arr.iter().any(|i| has_deep_property(i, property)),
        _ => false,
    }
}

// ref: rxdb/src/plugins/utils/utils-object.ts:239-268
/// Deeply checks if an object contains any property with the value of undefined.
/// In JSON there is no `undefined`, so we treat `Value::Null` the same.
/// If yes, returns the path to it.
pub fn find_undefined_path(obj: &Value, parent_path: &str) -> Option<String> {
    let map = obj.as_object()?;
    for (k, v) in map.iter() {
        let current = if parent_path.is_empty() {
            k.clone()
        } else {
            format!("{parent_path}.{k}")
        };
        if v.is_null() {
            return Some(current);
        }
        if v.is_object() {
            if let Some(found) = find_undefined_path(v, &current) {
                return Some(found);
            }
        }
    }
    None
}
