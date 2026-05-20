//! Custom errors with the additional field 'parameters'.
//!
//! T1 re-design: upstream has two JS classes (`RxError` extends Error, `RxTypeError`
//! extends TypeError). Rust collapses both into a single `thiserror` enum with a
//! `type_error()` discriminator, since Rust has no built-in `TypeError` distinction.
//! All upstream string error codes (`PL3`, `COL20`, etc.) are preserved verbatim.

use std::fmt;

use serde_json::{json, Value};

use crate::overwritable::OVERWRITABLE;

/// `Record<string, any>` in upstream. We use a JSON Value for shape-compatibility.
pub type RxErrorParameters = Value;
/// `RxErrorKey` in upstream is a TS union of literal error-code strings.
pub type RxErrorKey = String;

// ref: rxdb/src/rx-error.ts:51-111
#[derive(Debug, Clone)]
pub enum RxError {
    /// Maps to upstream `RxError` (extends Error).
    Standard {
        code: RxErrorKey,
        message: String,
        url: String,
        parameters: RxErrorParameters,
    },
    /// Maps to upstream `RxTypeError` (extends TypeError).
    Type {
        code: RxErrorKey,
        message: String,
        url: String,
        parameters: RxErrorParameters,
    },
}

impl RxError {
    pub fn code(&self) -> &str {
        match self {
            Self::Standard { code, .. } | Self::Type { code, .. } => code,
        }
    }
    pub fn url(&self) -> &str {
        match self {
            Self::Standard { url, .. } | Self::Type { url, .. } => url,
        }
    }
    pub fn parameters(&self) -> &RxErrorParameters {
        match self {
            Self::Standard { parameters, .. } | Self::Type { parameters, .. } => parameters,
        }
    }
    /// Mirrors upstream `RxError.typeError` getter.
    pub fn type_error(&self) -> bool {
        matches!(self, Self::Type { .. })
    }
    /// Mirrors upstream `name` getter.
    pub fn name(&self) -> String {
        match self {
            Self::Standard { code, .. } => format!("RxError ({code})"),
            Self::Type { code, .. } => format!("RxTypeError ({code})"),
        }
    }
}

impl fmt::Display for RxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::Standard { message, .. } | Self::Type { message, .. } => message,
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for RxError {}

pub type RxResult<T> = std::result::Result<T, RxError>;

// ref: rxdb/src/rx-error.ts:16-39
/// transform an object of parameters to a presentable string
fn parameters_to_string(parameters: &Value) -> String {
    let obj = match parameters.as_object() {
        Some(o) if !o.is_empty() => o,
        _ => return String::new(),
    };
    let mut ret = String::new();
    ret.push_str(&"-".repeat(20));
    ret.push('\n');
    ret.push_str("Parameters:\n");
    let lines: Vec<String> = obj
        .iter()
        .map(|(k, v)| {
            let param_str = if k == "errors" {
                // ref: rxdb/src/rx-error.ts:26-27
                // upstream: parameters[k].map((err) => JSON.stringify(err, Object.getOwnPropertyNames(err)))
                serde_json::to_string(v).unwrap_or_else(|_| "[object Object]".to_string())
            } else {
                // ref: rxdb/src/rx-error.ts:28-32
                // upstream: JSON.stringify(parameters[k], (_k, v) => v === undefined ? null : v, 2)
                serde_json::to_string_pretty(v).unwrap_or_else(|_| "[object Object]".to_string())
            };
            format!("{k}: {param_str}")
        })
        .collect();
    ret.push_str(&lines.join("\n"));
    ret.push('\n');
    ret
}

// ref: rxdb/src/rx-error.ts:41-49
fn message_for_error(message: &str, _code: &str, parameters: &Value) -> String {
    format!("\n{message}\n{}", parameters_to_string(parameters))
}

// ref: rxdb/src/rx-error.ts:114-116
pub fn get_error_url(code: &str) -> String {
    format!("https://rxdb.info/errors.html?console=errors#{code}")
}

// ref: rxdb/src/rx-error.ts:118-120
pub fn error_url_hint(code: &str) -> String {
    format!(
        "\nFind out more about this error here: {} \n",
        get_error_url(code)
    )
}

// ref: rxdb/src/rx-error.ts:122-131
pub fn new_rx_error(code: &str, parameters: Option<RxErrorParameters>) -> RxError {
    let params = parameters.unwrap_or_else(|| Value::Object(Default::default()));
    let tunnel = (OVERWRITABLE.load().tunnel_error_message)(code);
    let mes = message_for_error(&format!("{tunnel}{}", error_url_hint(code)), code, &params);
    RxError::Standard {
        code: code.to_string(),
        message: mes,
        url: get_error_url(code),
        parameters: params,
    }
}

// ref: rxdb/src/rx-error.ts:133-142
pub fn new_rx_type_error(code: &str, parameters: Option<RxErrorParameters>) -> RxError {
    let params = parameters.unwrap_or_else(|| Value::Object(Default::default()));
    let tunnel = (OVERWRITABLE.load().tunnel_error_message)(code);
    let mes = message_for_error(&format!("{tunnel}{}", error_url_hint(code)), code, &params);
    RxError::Type {
        code: code.to_string(),
        message: mes,
        url: get_error_url(code),
        parameters: params,
    }
}

// ref: rxdb/src/rx-error.ts:149-160
/// Returns Some(err) if it is a 409 conflict, None if it is another error.
pub fn is_bulk_write_conflict_error(err: &Value) -> Option<&Value> {
    if err.get("status").and_then(|v| v.as_u64()) == Some(409) {
        Some(err)
    } else {
        None
    }
}

// ref: rxdb/src/rx-error.ts:163-167
fn storage_write_error_code_to_message(status: u64) -> &'static str {
    match status {
        409 => "document write conflict",
        422 => "schema validation error",
        510 => "attachment data missing",
        _ => "",
    }
}

// ref: rxdb/src/rx-error.ts:169-175
pub fn rx_storage_write_error_to_rx_error(err: &Value) -> RxError {
    let status = err.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
    let document_id = err.get("documentId").cloned().unwrap_or(Value::Null);
    new_rx_error(
        "COL20",
        Some(json!({
            "name": storage_write_error_code_to_message(status),
            "document": document_id,
            "writeError": err,
        })),
    )
}
