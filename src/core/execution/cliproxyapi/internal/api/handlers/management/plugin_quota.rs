// ref: internal/api/handlers/management/plugin_quota.go:475-823 @ 0d18d3b365be000eeaee7de4615f9da596b2059d
// Port-Status: candidate
// Port-Note: pure normalization; credential resolution and HTTP remain host-owned
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::sdk::pluginapi::{QuotaBucket, QuotaFetchResponse, QuotaGroup, QuotaMetric};
use serde_json::{Map, Value};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuotaProbeError {
    InvalidJson,
    InvalidShape,
    ResponseTooLarge,
}
const MAX_QUOTA_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// A missing fraction is unknown, never a reported exhausted allowance.
pub fn normalize_quota_probe(
    raw: &[u8],
    mapping: Option<&Map<String, Value>>,
    server_offset_ms: i64,
) -> Result<QuotaFetchResponse, QuotaProbeError> {
    if raw.len() > MAX_QUOTA_RESPONSE_BYTES {
        return Err(QuotaProbeError::ResponseTooLarge);
    }
    let mut value: Value = serde_json::from_slice(raw).map_err(|_| QuotaProbeError::InvalidJson)?;
    let summary = usable_summary(&value);
    let mut out = if let Some(mapping) = mapping {
        map_quota(raw, mapping)?
    } else {
        let object = value.as_object_mut().ok_or(QuotaProbeError::InvalidShape)?;
        object.retain(|key, _| !summary_key(key));
        let mut out: QuotaFetchResponse =
            serde_json::from_value(value.clone()).map_err(|_| QuotaProbeError::InvalidShape)?;
        let raw_groups = value.get("groups").and_then(Value::as_array);
        out.groups = out
            .groups
            .into_iter()
            .enumerate()
            .filter_map(|(g, mut group)| {
                let buckets = raw_groups?.get(g)?.get("buckets")?.as_array()?;
                group.buckets = group
                    .buckets
                    .into_iter()
                    .enumerate()
                    .filter_map(|(b, mut bucket)| {
                        let raw = buckets.get(b)?;
                        let fraction = raw
                            .get("remainingFraction")
                            .or_else(|| raw.get("remaining_fraction"))?;
                        bucket.remaining_fraction = numeric_fraction(fraction)?;
                        Some(bucket)
                    })
                    .collect();
                (!group.buckets.is_empty()).then_some(group)
            })
            .collect();
        out
    };
    out.summary = summary;
    let has_plan = out
        .subscription
        .as_ref()
        .is_some_and(|s| !s.plan.trim().is_empty());
    if !has_plan && out.groups.is_empty() && out.summary.is_empty() {
        return Err(QuotaProbeError::InvalidShape);
    }
    if out.server_time_offset_ms == 0 {
        out.server_time_offset_ms = server_offset_ms;
    }
    Ok(out)
}

fn summary_key(key: &str) -> bool {
    key.chars()
        .map(|c| {
            if c == 'ſ' {
                's'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .eq("summary".chars())
}
fn text(value: Option<&Value>) -> &str {
    value.and_then(Value::as_str).unwrap_or_default()
}
fn numeric_fraction(value: &Value) -> Option<f64> {
    let number = if let Some(s) = value.as_str() {
        s.trim().parse::<f64>().ok()?
    } else {
        value.as_f64()?
    };
    number.is_finite().then_some(number)
}
fn currency_code(input: &str) -> Option<String> {
    // Go's unicode.ToUpper is a single-rune mapping, unlike expanding Rust uppercase.
    let mut code = String::new();
    for c in input.trim().chars() {
        let mut upper = c.to_uppercase();
        let first = upper.next()?;
        if upper.next().is_some() {
            return None;
        }
        code.push(first);
    }
    static CODES: OnceLock<Vec<String>> = OnceLock::new();
    // Generated with x/text v0.40.0 currency.ParseISO, the exact upstream dependency.
    // Its BSD licence is retained beside quota_currency_codes.json.
    let codes = CODES.get_or_init(|| {
        serde_json::from_str(include_str!("quota_currency_codes.json"))
            .expect("pinned ISO currency data")
    });
    codes.binary_search(&code).is_ok().then_some(code)
}
fn usable_summary(value: &Value) -> Vec<QuotaMetric> {
    let Some(object) = value.as_object() else {
        return vec![];
    };
    let raw = object.get("summary").or_else(|| {
        object
            .iter()
            .find(|(key, _)| summary_key(key))
            .map(|(_, v)| v)
    });
    raw.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|metric| {
            let key = metric.get("key")?.as_str()?.trim();
            let label = metric.get("label")?.as_str()?.trim();
            let value = metric.get("value")?.as_f64()?;
            if key.is_empty() || label.is_empty() || !value.is_finite() {
                return None;
            }
            let mut out = QuotaMetric {
                key: key.into(),
                label: label.into(),
                value,
                unit: text(metric.get("unit")).trim().into(),
                ..Default::default()
            };
            match text(metric.get("format")).trim() {
                "number" => out.format = "number".into(),
                "currency" => {
                    if let Some(code) = currency_code(text(metric.get("currency"))) {
                        out.format = "currency".into();
                        out.currency = code;
                    }
                }
                _ => {}
            }
            Some(out)
        })
        .collect()
}
fn path_value(document: &str, path: &str) -> Option<Value> {
    let value = gjson::get(document, path);
    value
        .exists()
        .then(|| serde_json::from_str(value.json()).ok())
        .flatten()
}
fn go_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}
fn mapped_text(document: &str, path: &str, literal: bool) -> String {
    path_value(document, path)
        .map(|v| go_string(&v))
        .unwrap_or_else(|| if literal { path.into() } else { String::new() })
}
fn fraction_at(
    document: &str,
    mapping: &Map<String, Value>,
    fraction: &str,
    remaining: &str,
    total: &str,
) -> Option<f64> {
    let path = text(mapping.get(fraction));
    if !path.is_empty() {
        if let Some(value) = path_value(document, path).and_then(|v| numeric_fraction(&v)) {
            return Some(value);
        }
    }
    let remaining = text(mapping.get(remaining));
    let total = text(mapping.get(total));
    if remaining.is_empty() || total.is_empty() {
        return None;
    }
    let left = numeric_fraction(&path_value(document, remaining)?)?;
    let size = numeric_fraction(&path_value(document, total)?)?;
    if size <= 0.0 {
        return None;
    }
    let ratio = left / size;
    // Go's json encoder rejects nonfinite ratios too; never turn overflow into zero.
    ratio.is_finite().then_some(ratio)
}
fn map_quota(
    raw: &[u8],
    mapping: &Map<String, Value>,
) -> Result<QuotaFetchResponse, QuotaProbeError> {
    let document = std::str::from_utf8(raw).map_err(|_| QuotaProbeError::InvalidJson)?;
    let mut out = QuotaFetchResponse::default();
    for (key, alias) in [
        ("plan", "plan"),
        ("tier_name", "tierName"),
        ("tier_id", "tierId"),
    ] {
        let path = text(mapping.get(key));
        let path = if path.is_empty() {
            text(mapping.get(alias))
        } else {
            path
        };
        if path.is_empty() {
            continue;
        }
        if let Some(value) = path_value(document, path) {
            let value = go_string(&value);
            if !value.trim().is_empty() {
                let subscription = out.subscription.get_or_insert_with(Default::default);
                match key {
                    "plan" => subscription.plan = value,
                    "tier_name" => subscription.tier_name = value,
                    _ => subscription.tier_id = value,
                }
            }
        }
    }
    for group_mapping in mapping
        .get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(gm) = group_mapping.as_object() else {
            continue;
        };
        let mut group = QuotaGroup::default();
        if let Some(path) = gm
            .get("display_name")
            .and_then(Value::as_str)
            .or_else(|| gm.get("displayName").and_then(Value::as_str))
        {
            let name = mapped_text(document, path, false);
            group.display_name = if name.trim().is_empty() {
                path.into()
            } else {
                name
            };
        }
        let buckets_path = text(gm.get("buckets_path"));
        if !buckets_path.is_empty() {
            if let Some(Value::Array(items)) = path_value(document, buckets_path) {
                for item in items {
                    let item = item.to_string();
                    let mut paths = Map::new();
                    for (key, default) in [
                        ("remaining_fraction_key", "remaining_fraction"),
                        ("remaining_amount_key", ""),
                        ("total_amount_key", ""),
                    ] {
                        let path = text(gm.get(key));
                        paths.insert(
                            key.into(),
                            Value::String(if path.is_empty() { default } else { path }.into()),
                        );
                    }
                    let Some(fraction) = fraction_at(
                        &item,
                        &paths,
                        "remaining_fraction_key",
                        "remaining_amount_key",
                        "total_amount_key",
                    ) else {
                        continue;
                    };
                    let read = |key, default| {
                        let path = text(gm.get(key));
                        mapped_text(&item, if path.is_empty() { default } else { path }, false)
                    };
                    group.buckets.push(QuotaBucket {
                        window: read("window_key", "window"),
                        remaining_fraction: fraction,
                        reset_time: read("reset_time_key", "reset_time"),
                        description: read("description_key", "description"),
                    });
                }
            }
        }
        for bucket_mapping in gm
            .get("buckets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(bm) = bucket_mapping.as_object() else {
                continue;
            };
            let Some(fraction) = fraction_at(
                document,
                bm,
                "remaining_fraction",
                "remaining_amount",
                "total_amount",
            ) else {
                continue;
            };
            let read = |key| {
                bm.get(key)
                    .and_then(Value::as_str)
                    .map(|path| mapped_text(document, path, true))
                    .unwrap_or_default()
            };
            group.buckets.push(QuotaBucket {
                remaining_fraction: fraction,
                window: read("window"),
                reset_time: read("reset_time"),
                description: read("description"),
            });
        }
        if !group.buckets.is_empty() {
            out.groups.push(group);
        }
    }
    Ok(out)
}
