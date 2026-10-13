//! Shared helpers for declarative toolkit definitions.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::model::{LocalizedText, ToolSpec};

pub type Props<'a> = &'a [(&'a str, Value)];

#[allow(clippy::too_many_arguments)]
pub fn tool(
    toolkit: &str,
    slug: &str,
    title: (&str, &str),
    desc: (&str, &str),
    props: Props<'_>,
    required: &[&str],
    scopes: &[&str],
    mutating: bool,
) -> ToolSpec {
    let mut m = Map::new();
    for (k, v) in props {
        m.insert((*k).into(), v.clone());
    }
    ToolSpec {
        slug: slug.into(),
        toolkit: toolkit.into(),
        title: LocalizedText::new(title.0, title.1),
        description: LocalizedText::new(desc.0, desc.1),
        input_schema: json!({"type": "object", "properties": m, "required": required, "additionalProperties": false}),
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
        mutating,
    }
}

pub fn s(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}
pub fn int(desc: &str, min: u64, max: u64, default: u64) -> Value {
    json!({"type": "integer", "description": desc, "minimum": min, "maximum": max, "default": default})
}
pub fn str_list(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": desc})
}

pub fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| anyhow!("missing argument `{key}`"))
}
pub fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
}
pub fn opt_u64(args: &Value, key: &str, default: u64, min: u64, max: u64) -> u64 {
    args.get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default)
        .clamp(min, max)
}
/// String list; a single string is accepted as one element.
pub fn str_vec(args: &Value, key: &str) -> Vec<String> {
    match args.get(key) {
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

/// Reject header-injection characters in values that end up in MIME headers.
pub fn header_safe<'a>(key: &str, v: &'a str) -> Result<&'a str> {
    if v.contains('\r') || v.contains('\n') {
        bail!("invalid argument `{key}`");
    }
    Ok(v)
}

/// `?a=b&c=d` from optional pairs (percent-encoded).
pub fn query(pairs: &[(&str, Option<String>)]) -> String {
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    let mut any = false;
    for (k, v) in pairs {
        if let Some(v) = v {
            ser.append_pair(k, v);
            any = true;
        }
    }
    if any {
        format!("?{}", ser.finish())
    } else {
        String::new()
    }
}
