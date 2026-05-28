//! Canonical query fingerprint shared between JS and Rust.
//!
//! Mirrors `src/apps/business-os/rxdb/src/query-fingerprint.mjs` byte-for-byte.
//! The corpus under `tests/fixtures/query_fingerprint/` is the cross-language
//! parity gate: both implementations MUST produce identical canonical JSON
//! and SHA-256 fingerprints for each fixture.

use std::cmp::Ordering;
use std::fmt::Write as _;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub const PROTOCOL_VERSION: &str = "1.5";

#[derive(Debug, thiserror::Error)]
pub enum FingerprintError {
    #[error("query input must be an object")]
    NotAnObject,
    #[error("collection is required")]
    MissingCollection,
    #[error("selector must be a plain object")]
    InvalidSelector,
    #[error("sort must be an array of single-key direction objects")]
    InvalidSort,
    #[error("sort entries must have exactly one key")]
    InvalidSortEntry,
    #[error("invalid sort direction: {0}")]
    InvalidSortDirection(String),
    #[error("optional number must be a non-negative finite value")]
    InvalidOptionalNumber,
    #[error("window must be an object")]
    InvalidWindow,
}

pub fn canonicalize_query_input(input: &Value) -> Result<Value, FingerprintError> {
    let obj = input.as_object().ok_or(FingerprintError::NotAnObject)?;

    let collection = obj
        .get("collection")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(FingerprintError::MissingCollection)?
        .to_string();

    let schema_version = obj
        .get("schemaVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let selector = canonicalize_selector(obj.get("selector").unwrap_or(&Value::Null))?;
    let sort = canonicalize_sort(obj.get("sort").unwrap_or(&Value::Null))?;
    let limit = canonicalize_optional_number(obj.get("limit").unwrap_or(&Value::Null))?;
    let skip = canonicalize_optional_number(obj.get("skip").unwrap_or(&Value::Null))?;
    let window = canonicalize_window(obj.get("window").unwrap_or(&Value::Null))?;

    let mut out = Map::new();
    out.insert("collection".into(), Value::String(collection));
    out.insert("limit".into(), limit);
    out.insert(
        "protocolVersion".into(),
        Value::String(PROTOCOL_VERSION.into()),
    );
    out.insert("schemaVersion".into(), Value::from(schema_version));
    out.insert("selector".into(), selector);
    out.insert("skip".into(), skip);
    out.insert("sort".into(), sort);
    out.insert("window".into(), window);
    Ok(Value::Object(out))
}

pub fn canonical_query_json(input: &Value) -> Result<String, FingerprintError> {
    let canonical = canonicalize_query_input(input)?;
    Ok(emit_canonical(&canonical))
}

pub fn query_fingerprint(input: &Value) -> Result<String, FingerprintError> {
    let bytes = canonical_query_json(input)?;
    let digest = Sha256::digest(bytes.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        write!(&mut hex, "{:02x}", byte).expect("write to String never fails");
    }
    Ok(hex)
}

fn canonicalize_selector(value: &Value) -> Result<Value, FingerprintError> {
    if value.is_null() {
        return Ok(Value::Object(Map::new()));
    }
    if !value.is_object() {
        return Err(FingerprintError::InvalidSelector);
    }
    Ok(canonicalize_selector_value(value))
}

fn canonicalize_selector_value(value: &Value) -> Value {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => value.clone(),
        Value::Array(items) => {
            Value::Array(items.iter().map(canonicalize_selector_value).collect())
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::with_capacity(map.len());
            for key in keys {
                let next = canonicalize_selector_value(&map[key]);
                if key == "$in" || key == "$nin" {
                    out.insert(key.clone(), sort_and_dedupe_array(next));
                } else {
                    out.insert(key.clone(), next);
                }
            }
            Value::Object(out)
        }
    }
}

fn sort_and_dedupe_array(value: Value) -> Value {
    if let Value::Array(items) = value {
        let mut keyed: Vec<(String, Value)> = items
            .into_iter()
            .map(|item| (emit_canonical(&item), item))
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        keyed.dedup_by(|a, b| a.0 == b.0);
        Value::Array(keyed.into_iter().map(|(_, item)| item).collect())
    } else {
        value
    }
}

fn canonicalize_sort(value: &Value) -> Result<Value, FingerprintError> {
    if value.is_null() {
        return Ok(Value::Array(Vec::new()));
    }
    let arr = value.as_array().ok_or(FingerprintError::InvalidSort)?;
    let mut out = Vec::with_capacity(arr.len());
    for entry in arr {
        let obj = entry
            .as_object()
            .ok_or(FingerprintError::InvalidSortEntry)?;
        if obj.len() != 1 {
            return Err(FingerprintError::InvalidSortEntry);
        }
        let (key, direction) = obj.iter().next().unwrap();
        let normalized = normalize_sort_direction(direction)?;
        let mut entry_obj = Map::new();
        entry_obj.insert(key.clone(), Value::String(normalized));
        out.push(Value::Object(entry_obj));
    }
    Ok(Value::Array(out))
}

fn normalize_sort_direction(direction: &Value) -> Result<String, FingerprintError> {
    match direction {
        Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "asc" | "1" => Ok("asc".to_string()),
            "desc" | "-1" => Ok("desc".to_string()),
            _ => Err(FingerprintError::InvalidSortDirection(s.clone())),
        },
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                match i {
                    1 => Ok("asc".to_string()),
                    -1 => Ok("desc".to_string()),
                    _ => Err(FingerprintError::InvalidSortDirection(i.to_string())),
                }
            } else {
                Err(FingerprintError::InvalidSortDirection(n.to_string()))
            }
        }
        other => Err(FingerprintError::InvalidSortDirection(other.to_string())),
    }
}

fn canonicalize_optional_number(value: &Value) -> Result<Value, FingerprintError> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    let n = value
        .as_f64()
        .ok_or(FingerprintError::InvalidOptionalNumber)?;
    if !n.is_finite() || n < 0.0 {
        return Err(FingerprintError::InvalidOptionalNumber);
    }
    Ok(Value::from(n.floor() as i64))
}

fn canonicalize_window(value: &Value) -> Result<Value, FingerprintError> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    let obj = value.as_object().ok_or(FingerprintError::InvalidWindow)?;
    let offset = canonicalize_optional_number(obj.get("offset").unwrap_or(&Value::Null))?;
    let offset = if offset.is_null() {
        Value::from(0i64)
    } else {
        offset
    };
    let limit = canonicalize_optional_number(obj.get("limit").unwrap_or(&Value::Null))?;
    let limit = if limit.is_null() {
        Value::from(200i64)
    } else {
        limit
    };
    let mut out = Map::new();
    out.insert("limit".into(), limit);
    out.insert("offset".into(), offset);
    Ok(Value::Object(out))
}

fn emit_canonical(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value);
    out
}

fn write_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| match a.cmp(b) {
                Ordering::Equal => Ordering::Equal,
                other => other,
            });
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, key);
                out.push(':');
                write_value(out, &map[*key]);
            }
            out.push('}');
        }
    }
}

fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn corpus_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("query_fingerprint")
    }

    #[test]
    fn corpus_matches_js() {
        let dir = corpus_dir();
        let mut entries: Vec<_> = fs::read_dir(&dir)
            .unwrap_or_else(|err| panic!("read_dir {:?}: {err}", dir))
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        entries.sort();
        assert!(
            !entries.is_empty(),
            "expected fingerprint corpus to contain fixtures"
        );
        for path in &entries {
            let text = fs::read_to_string(path).expect("read corpus fixture");
            let fixture: Value = serde_json::from_str(&text).expect("parse fixture json");
            let input = fixture.get("input").expect("fixture must have input");
            let expected_canonical = fixture
                .get("canonicalJson")
                .and_then(Value::as_str)
                .expect("fixture must have canonicalJson");
            let expected_fingerprint = fixture
                .get("fingerprint")
                .and_then(Value::as_str)
                .expect("fixture must have fingerprint");

            let canonical = canonical_query_json(input).expect("canonicalize must succeed");
            assert_eq!(
                canonical,
                expected_canonical,
                "canonical JSON mismatch for {}",
                path.display()
            );
            let fingerprint = query_fingerprint(input).expect("fingerprint must succeed");
            assert_eq!(
                fingerprint,
                expected_fingerprint,
                "fingerprint mismatch for {}",
                path.display()
            );
        }
    }
}
