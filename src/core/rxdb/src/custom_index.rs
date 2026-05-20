//! Port of `src/custom-index.ts`.
//!
//! For some RxStorage implementations, we need to use our custom crafted indexes
//! so we can easily iterate over them. And sort plain arrays of document data.
//!
//! Performance is critical here; the upstream uses closures captured up-front
//! for monad-style amortisation. The Rust port mirrors that with `Box<dyn Fn>`.

use serde_json::Value;

use crate::plugins::utils::utils_object::object_path_monad;
use crate::plugins::utils::utils_other::ensure_not_falsy;
use crate::query_planner::{INDEX_MAX, INDEX_MIN};
use crate::rx_error::{new_rx_error, RxResult};
use crate::rx_schema_helper::get_schema_by_object_path;
use crate::types::{JsonSchema, RxJsonSchema};

// ref: rxdb/src/custom-index.ts:38-47
pub struct IndexMetaField {
    pub field_name: String,
    pub schema_part: JsonSchema,
    /// Only set for number-typed index fields.
    pub parsed_lengths: Option<ParsedLengths>,
    pub get_value: Box<dyn Fn(&Value) -> Value + Send + Sync>,
    pub get_index_string_part: Box<dyn Fn(&Value) -> String + Send + Sync>,
}

// ref: rxdb/src/custom-index.ts:143-149
#[derive(Debug, Clone, Copy)]
pub struct ParsedLengths {
    pub minimum: i64,
    pub maximum: i64,
    pub non_decimals: usize,
    pub decimals: usize,
    pub rounded_minimum: i64,
}

// ref: rxdb/src/custom-index.ts:49-106
pub fn get_index_meta(schema: &RxJsonSchema, index: &[String]) -> RxResult<Vec<IndexMetaField>> {
    let mut out = Vec::with_capacity(index.len());
    for field_name in index.iter() {
        let schema_part = get_schema_by_object_path(schema, field_name);
        if schema_part.schema_type.is_none() {
            return Err(new_rx_error(
                "UTL6",
                Some(serde_json::json!({ "message": format!("not in schema: {field_name}") })),
            ));
        }
        let ty = schema_part.schema_type.clone().unwrap_or_default();
        let parsed_lengths = if ty == "number" || ty == "integer" {
            Some(get_string_length_of_index_number(&schema_part)?)
        } else {
            None
        };
        let get_value = object_path_monad(field_name);
        let max_length = schema_part.max_length.unwrap_or(0) as usize;
        let ty_clone = ty.clone();
        let get_value_for_str = object_path_monad(field_name);
        let get_value_for_bool = object_path_monad(field_name);
        let get_value_for_num = object_path_monad(field_name);
        let parsed_lengths_for_num = parsed_lengths;

        let get_index_string_part: Box<dyn Fn(&Value) -> String + Send + Sync> =
            match ty_clone.as_str() {
                "string" => Box::new(move |doc| {
                    let fv = get_value_for_str(doc);
                    let s = match &fv {
                        Value::String(s) => s.clone(),
                        Value::Null => String::new(),
                        other if other.is_null() => String::new(),
                        other => other.to_string(),
                    };
                    pad_end(&s, max_length, ' ')
                }),
                "boolean" => Box::new(move |doc| {
                    let fv = get_value_for_bool(doc);
                    if fv.as_bool().unwrap_or(false) {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }
                }),
                _ => Box::new(move |doc| {
                    let fv = get_value_for_num(doc);
                    get_number_index_string(
                        &parsed_lengths_for_num.expect("number field without parsed_lengths"),
                        fv.as_f64(),
                    )
                }),
            };

        out.push(IndexMetaField {
            field_name: field_name.clone(),
            schema_part,
            parsed_lengths,
            get_value,
            get_index_string_part,
        });
    }
    Ok(out)
}

// ref: rxdb/src/custom-index.ts:120-140
/// Crafts an indexable string that can be used to check if a document would be
/// sorted below or above another document, dependent on the index values.
pub fn get_indexable_string_monad(
    schema: &RxJsonSchema,
    index: &[String],
) -> RxResult<Box<dyn Fn(&Value) -> String + Send + Sync>> {
    let meta = get_index_meta(schema, index)?;
    Ok(Box::new(move |doc| {
        let mut s = String::new();
        for f in meta.iter() {
            s.push_str(&(f.get_index_string_part)(doc));
        }
        s
    }))
}

// ref: rxdb/src/custom-index.ts:150-172
pub fn get_string_length_of_index_number(schema_part: &JsonSchema) -> RxResult<ParsedLengths> {
    let minimum = schema_part.minimum.unwrap_or(0.0).floor() as i64;
    let maximum = schema_part.maximum.unwrap_or(0.0).ceil() as i64;
    let multiple_of = schema_part.multiple_of.unwrap_or(0.0);
    let value_span = maximum - minimum;
    let non_decimals = value_span.to_string().len();
    let multiple_of_str = multiple_of.to_string();
    let parts: Vec<&str> = multiple_of_str.split('.').collect();
    let decimals = if parts.len() > 1 { parts[1].len() } else { 0 };
    Ok(ParsedLengths {
        minimum,
        maximum,
        non_decimals,
        decimals,
        rounded_minimum: minimum,
    })
}

// ref: rxdb/src/custom-index.ts:174-195
pub fn get_index_string_length(schema: &RxJsonSchema, index: &[String]) -> RxResult<usize> {
    let meta = get_index_meta(schema, index)?;
    let mut length = 0;
    for props in meta.iter() {
        let ty = props.schema_part.schema_type.as_deref().unwrap_or("");
        match ty {
            "string" => length += props.schema_part.max_length.unwrap_or(0) as usize,
            "boolean" => length += 1,
            _ => {
                let pl = props.parsed_lengths.unwrap();
                length += pl.non_decimals + pl.decimals;
            }
        }
    }
    Ok(length)
}

// ref: rxdb/src/custom-index.ts:198-206
pub fn get_primary_key_from_indexable_string(
    indexable_string: &str,
    primary_key_length: usize,
) -> String {
    let len = indexable_string.chars().count();
    let start = len.saturating_sub(primary_key_length);
    let padded: String = indexable_string.chars().skip(start).collect();
    padded.trim().to_string()
}

// ref: rxdb/src/custom-index.ts:209-241
pub fn get_number_index_string(parsed_lengths: &ParsedLengths, field_value: Option<f64>) -> String {
    let mut fv = field_value.unwrap_or(0.0);
    if (fv as i64) < parsed_lengths.minimum {
        fv = parsed_lengths.minimum as f64;
    }
    if (fv as i64) > parsed_lengths.maximum {
        fv = parsed_lengths.maximum as f64;
    }
    let non_decimals_value = (fv.floor() as i64) - parsed_lengths.rounded_minimum;
    let mut s = format!(
        "{:0>width$}",
        non_decimals_value,
        width = parsed_lengths.non_decimals
    );
    if parsed_lengths.decimals > 0 {
        let fv_str = fv.to_string();
        let parts: Vec<&str> = fv_str.split('.').collect();
        let decimal_str = if parts.len() > 1 { parts[1] } else { "0" };
        let padded = pad_end(decimal_str, parsed_lengths.decimals, '0');
        s.push_str(&padded);
    }
    s
}

// ref: rxdb/src/custom-index.ts:243-305
pub fn get_start_index_string_from_lower_bound(
    schema: &RxJsonSchema,
    index: &[String],
    lower_bound: &[Value],
) -> RxResult<String> {
    let mut out = String::new();
    for (idx, field_name) in index.iter().enumerate() {
        let schema_part = get_schema_by_object_path(schema, field_name);
        let bound = lower_bound.get(idx).cloned().unwrap_or(Value::Null);
        let ty = schema_part.schema_type.as_deref().unwrap_or("");
        match ty {
            "string" => {
                let max_length =
                    ensure_not_falsy(schema_part.max_length, Some("maxLength not set"))? as usize;
                if let Some(s) = bound.as_str() {
                    out.push_str(&pad_end(s, max_length, ' '));
                } else {
                    out.push_str(&pad_end("", max_length, ' '));
                }
            }
            "boolean" => {
                if bound.is_null() || bound == *INDEX_MIN {
                    out.push('0');
                } else if bound == *INDEX_MAX {
                    out.push('1');
                } else {
                    out.push(if bound.as_bool().unwrap_or(false) {
                        '1'
                    } else {
                        '0'
                    });
                }
            }
            "number" | "integer" => {
                let parsed_lengths = get_string_length_of_index_number(&schema_part)?;
                if bound.is_null() || bound == *INDEX_MIN {
                    for _ in 0..(parsed_lengths.non_decimals + parsed_lengths.decimals) {
                        out.push('0');
                    }
                } else if bound == *INDEX_MAX {
                    out.push_str(&get_number_index_string(
                        &parsed_lengths,
                        Some(parsed_lengths.maximum as f64),
                    ));
                } else {
                    out.push_str(&get_number_index_string(&parsed_lengths, bound.as_f64()));
                }
            }
            other => {
                return Err(new_rx_error(
                    "UTL7",
                    Some(serde_json::json!({ "message": format!("unknown index type {other}") })),
                ));
            }
        }
    }
    Ok(out)
}

// ref: rxdb/src/custom-index.ts:308-364
pub fn get_start_index_string_from_upper_bound(
    schema: &RxJsonSchema,
    index: &[String],
    upper_bound: &[Value],
) -> RxResult<String> {
    let mut out = String::new();
    for (idx, field_name) in index.iter().enumerate() {
        let schema_part = get_schema_by_object_path(schema, field_name);
        let bound = upper_bound.get(idx).cloned().unwrap_or(Value::Null);
        let ty = schema_part.schema_type.as_deref().unwrap_or("");
        match ty {
            "string" => {
                let max_length =
                    ensure_not_falsy(schema_part.max_length, Some("maxLength not set"))? as usize;
                if bound.is_string() && bound != *INDEX_MAX {
                    out.push_str(&pad_end(bound.as_str().unwrap(), max_length, ' '));
                } else if bound == *INDEX_MIN {
                    out.push_str(&pad_end("", max_length, ' '));
                } else {
                    out.push_str(&pad_end("", max_length, '\u{ffff}'));
                }
            }
            "boolean" => {
                if bound.is_null() {
                    out.push('1');
                } else {
                    out.push(if bound.as_bool().unwrap_or(false) {
                        '1'
                    } else {
                        '0'
                    });
                }
            }
            "number" | "integer" => {
                let parsed_lengths = get_string_length_of_index_number(&schema_part)?;
                if bound.is_null() || bound == *INDEX_MAX {
                    for _ in 0..(parsed_lengths.non_decimals + parsed_lengths.decimals) {
                        out.push('9');
                    }
                } else if bound == *INDEX_MIN {
                    for _ in 0..(parsed_lengths.non_decimals + parsed_lengths.decimals) {
                        out.push('0');
                    }
                } else {
                    out.push_str(&get_number_index_string(&parsed_lengths, bound.as_f64()));
                }
            }
            other => {
                return Err(new_rx_error(
                    "UTL7",
                    Some(serde_json::json!({ "message": format!("unknown index type {other}") })),
                ));
            }
        }
    }
    Ok(out)
}

// ref: rxdb/src/custom-index.ts:370-376
/// Used in storages where it is not possible to define inclusiveEnd/inclusiveStart.
pub fn change_indexable_string_by_one_quantum(s: &str, direction: i32) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let last = *chars.last().unwrap();
    let new_code = (last as i32 + direction) as u32;
    let new_char = char::from_u32(new_code).unwrap_or(last);
    let mut out: String = chars[..chars.len() - 1].iter().collect();
    out.push(new_char);
    out
}

/// String padding analogue of JS `String.prototype.padEnd`.
fn pad_end(s: &str, target_len: usize, pad_char: char) -> String {
    let current_len = s.chars().count();
    if current_len >= target_len {
        return s.to_string();
    }
    let mut out = String::from(s);
    for _ in current_len..target_len {
        out.push(pad_char);
    }
    out
}
