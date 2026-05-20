//! Dot-prop path access.
//!
//! Copied from upstream which copied it from sindresorhus/dot-prop because
//! that package is ESM-only and caused optimization bailouts in JS.
//! Same algorithm here, translated to Rust on `serde_json::Value`.

use std::collections::HashSet;
use std::sync::LazyLock;

use serde_json::{json, Map, Value};

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:8-11
fn is_object(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Array(_))
}

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:13-17
static DISALLOWED_KEYS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::new();
    s.insert("__proto__");
    s.insert("prototype");
    s.insert("constructor");
    s
});

#[derive(Debug, Clone)]
pub enum PathSegment {
    Property(String),
    Index(usize),
}

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:21-168
fn get_path_segments(path: &str) -> Vec<PathSegment> {
    let mut parts: Vec<PathSegment> = Vec::new();
    let mut current_segment = String::new();
    let mut current_part = "start"; // "start" | "property" | "index" | "indexEnd"
    let mut is_ignoring = false;

    for character in path.chars() {
        match character {
            '\\' => {
                if current_part == "index" || current_part == "indexEnd" {
                    return Vec::new();
                }
                if is_ignoring {
                    current_segment.push(character);
                }
                current_part = "property";
                is_ignoring = !is_ignoring;
            }
            '.' => {
                if current_part == "index" {
                    return Vec::new();
                }
                if current_part == "indexEnd" {
                    current_part = "property";
                    continue;
                }
                if is_ignoring {
                    is_ignoring = false;
                    current_segment.push(character);
                    continue;
                }
                if DISALLOWED_KEYS.contains(current_segment.as_str()) {
                    return Vec::new();
                }
                parts.push(PathSegment::Property(std::mem::take(&mut current_segment)));
                current_part = "property";
            }
            '[' => {
                if current_part == "index" {
                    return Vec::new();
                }
                if current_part == "indexEnd" {
                    current_part = "index";
                    continue;
                }
                if is_ignoring {
                    is_ignoring = false;
                    current_segment.push(character);
                    continue;
                }
                if current_part == "property" {
                    if DISALLOWED_KEYS.contains(current_segment.as_str()) {
                        return Vec::new();
                    }
                    parts.push(PathSegment::Property(std::mem::take(&mut current_segment)));
                }
                current_part = "index";
            }
            ']' => {
                if current_part == "index" {
                    if let Ok(n) = current_segment.parse::<usize>() {
                        parts.push(PathSegment::Index(n));
                    } else {
                        return Vec::new();
                    }
                    current_segment.clear();
                    current_part = "indexEnd";
                } else if current_part == "indexEnd" {
                    return Vec::new();
                } else {
                    // falls through (upstream relies on switch fallthrough into default)
                    if current_part == "index" && !character.is_ascii_digit() {
                        return Vec::new();
                    }
                    if current_part == "indexEnd" {
                        return Vec::new();
                    }
                    if current_part == "start" {
                        current_part = "property";
                    }
                    if is_ignoring {
                        is_ignoring = false;
                        current_segment.push('\\');
                    }
                    current_segment.push(character);
                }
            }
            _ => {
                if current_part == "index" && !character.is_ascii_digit() {
                    return Vec::new();
                }
                if current_part == "indexEnd" {
                    return Vec::new();
                }
                if current_part == "start" {
                    current_part = "property";
                }
                if is_ignoring {
                    is_ignoring = false;
                    current_segment.push('\\');
                }
                current_segment.push(character);
            }
        }
    }

    if is_ignoring {
        current_segment.push('\\');
    }

    match current_part {
        "property" => {
            if DISALLOWED_KEYS.contains(current_segment.as_str()) {
                return Vec::new();
            }
            parts.push(PathSegment::Property(current_segment));
        }
        "index" => return Vec::new(),
        "start" => parts.push(PathSegment::Property(String::new())),
        _ => {}
    }
    parts
}

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:188-238
pub fn get_property(object: &Value, path: &str, default: Option<Value>) -> Value {
    // Performance shortcut.
    if !path.contains('.') && !path.contains('[') {
        if let Some(v) = object.get(path) {
            return v.clone();
        }
        return default.unwrap_or(Value::Null);
    }
    if !is_object(object) {
        return default.unwrap_or(object.clone());
    }
    let path_array = get_path_segments(path);
    if path_array.is_empty() {
        return default.unwrap_or(Value::Null);
    }
    let mut current: Value = object.clone();
    for (index, segment) in path_array.iter().enumerate() {
        let next = match segment {
            PathSegment::Property(k) => current.get(k).cloned(),
            PathSegment::Index(i) => current.get(*i).cloned(),
        };
        match next {
            Some(v) => current = v,
            None => {
                if index != path_array.len() - 1 {
                    return default.unwrap_or(Value::Null);
                }
                return default.unwrap_or(Value::Null);
            }
        }
    }
    current
}

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:240-267
pub fn set_property(object: &mut Value, path: &str, value: Value) {
    if !is_object(object) {
        return;
    }
    let path_array = get_path_segments(path);
    if path_array.is_empty() {
        return;
    }
    let mut current: &mut Value = object;
    for (index, segment) in path_array.iter().enumerate() {
        let is_last = index == path_array.len() - 1;
        let next_is_index = !is_last && matches!(path_array[index + 1], PathSegment::Index(_));
        match segment {
            PathSegment::Property(k) => {
                if !current.is_object() {
                    *current = Value::Object(Map::new());
                }
                let map = current.as_object_mut().unwrap();
                if is_last {
                    map.insert(k.clone(), value);
                    return;
                }
                if !map.contains_key(k) || !is_object(&map[k]) {
                    map.insert(
                        k.clone(),
                        if next_is_index {
                            Value::Array(Vec::new())
                        } else {
                            Value::Object(Map::new())
                        },
                    );
                }
                current = map.get_mut(k).unwrap();
            }
            PathSegment::Index(i) => {
                if !current.is_array() {
                    *current = Value::Array(Vec::new());
                }
                let arr = current.as_array_mut().unwrap();
                while arr.len() <= *i {
                    arr.push(Value::Null);
                }
                if is_last {
                    arr[*i] = value;
                    return;
                }
                if !is_object(&arr[*i]) {
                    arr[*i] = if next_is_index {
                        Value::Array(Vec::new())
                    } else {
                        Value::Object(Map::new())
                    };
                }
                current = &mut arr[*i];
            }
        }
    }
}

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:269-292
pub fn delete_property(object: &mut Value, path: &str) -> bool {
    if !is_object(object) {
        return false;
    }
    let path_array = get_path_segments(path);
    let mut current: &mut Value = object;
    for (index, segment) in path_array.iter().enumerate() {
        let is_last = index == path_array.len() - 1;
        match segment {
            PathSegment::Property(k) => {
                let Some(map) = current.as_object_mut() else {
                    return false;
                };
                if is_last {
                    return map.remove(k).is_some();
                }
                let Some(next) = map.get_mut(k) else {
                    return false;
                };
                if !is_object(next) {
                    return false;
                }
                current = next;
            }
            PathSegment::Index(i) => {
                let Some(arr) = current.as_array_mut() else {
                    return false;
                };
                if is_last {
                    if *i < arr.len() {
                        arr.remove(*i);
                        return true;
                    }
                    return false;
                }
                let Some(next) = arr.get_mut(*i) else {
                    return false;
                };
                if !is_object(next) {
                    return false;
                }
                current = next;
            }
        }
    }
    false
}

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:294-313
pub fn has_property(object: &Value, path: &str) -> bool {
    if !is_object(object) {
        return false;
    }
    let path_array = get_path_segments(path);
    if path_array.is_empty() {
        return false;
    }
    let mut current: &Value = object;
    for segment in path_array.iter() {
        let next = match segment {
            PathSegment::Property(k) => match current.as_object() {
                Some(map) if map.contains_key(k) => map.get(k),
                _ => return false,
            },
            PathSegment::Index(i) => current.get(*i),
        };
        match next {
            Some(v) => current = v,
            None => return false,
        }
    }
    true
}

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:349-365
fn deep_keys_recursive(object: &Value, current_path: &mut Vec<PathSegment>, out: &mut Vec<String>) {
    if !is_object(object) {
        if !current_path.is_empty() {
            out.push(stringify_path(current_path));
        }
        return;
    }
    match object {
        Value::Object(map) => {
            for (k, v) in map.iter() {
                current_path.push(PathSegment::Property(k.clone()));
                deep_keys_recursive(v, current_path, out);
                current_path.pop();
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                current_path.push(PathSegment::Index(i));
                deep_keys_recursive(v, current_path, out);
                current_path.pop();
            }
        }
        _ => {}
    }
}

pub fn deep_keys(object: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    deep_keys_recursive(object, &mut path, &mut out);
    out
}

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:316-322
fn escape_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if matches!(c, '\\' | '.' | '[') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// ref: rxdb/src/plugins/utils/utils-object-dot-prop.ts:333-347
fn stringify_path(path_segments: &[PathSegment]) -> String {
    let mut result = String::new();
    for (index, segment) in path_segments.iter().enumerate() {
        match segment {
            PathSegment::Index(i) => {
                result.push_str(&format!("[{i}]"));
            }
            PathSegment::Property(s) => {
                let escaped = escape_path(s);
                if index == 0 {
                    result.push_str(&escaped);
                } else {
                    result.push('.');
                    result.push_str(&escaped);
                }
            }
        }
    }
    let _ = json!(""); // silence unused-imports warning for `json`
    result
}
