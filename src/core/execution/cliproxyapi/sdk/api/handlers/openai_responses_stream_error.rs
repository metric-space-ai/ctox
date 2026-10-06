// ref: sdk/api/handlers/openai_responses_stream_error.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only
//
// Body matches candidate e2bff0107bb307337aaa19018ccddd55f64253d5. The line-1
// anchor stays on the accepted pin until promotion rewrites it.

use serde::Serialize;
use serde_json::{Map, Value};

const CLIENT_ERROR_TYPE: &str = "invalid_request_error";
const SERVER_ERROR_TYPE: &str = "server_error";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ErrorClass {
    code: &'static str,
    error_type: &'static str,
}

/// Status 408 is a transport failure of a well-formed request. Its code is
/// `request_timeout`, but its type must be `server_error` so clients retry.
/// Pairing that code with `invalid_request_error` tells clients the request
/// was malformed and must not be replayed.
fn error_class_for(status: u16) -> ErrorClass {
    match status {
        401 => ErrorClass {
            code: "invalid_api_key",
            error_type: CLIENT_ERROR_TYPE,
        },
        403 => ErrorClass {
            code: "insufficient_quota",
            error_type: CLIENT_ERROR_TYPE,
        },
        429 => ErrorClass {
            code: "rate_limit_exceeded",
            error_type: CLIENT_ERROR_TYPE,
        },
        404 => ErrorClass {
            code: "model_not_found",
            error_type: CLIENT_ERROR_TYPE,
        },
        408 => ErrorClass {
            code: "request_timeout",
            error_type: SERVER_ERROR_TYPE,
        },
        500.. => ErrorClass {
            code: "internal_server_error",
            error_type: SERVER_ERROR_TYPE,
        },
        400.. => ErrorClass {
            code: "invalid_request_error",
            error_type: CLIENT_ERROR_TYPE,
        },
        _ => ErrorClass {
            code: "unknown_error",
            error_type: CLIENT_ERROR_TYPE,
        },
    }
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesStreamErrorChunk {
    #[serde(rename = "type")]
    chunk_type: &'static str,
    error: Value,
    sequence_number: i64,
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesStreamFailedChunk {
    #[serde(rename = "type")]
    chunk_type: &'static str,
    sequence_number: i64,
    response: OpenAiResponsesStreamFailedResponse,
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesStreamFailedResponse {
    status: &'static str,
    error: Value,
}

pub fn build_openai_responses_stream_error_chunk(
    status: u16,
    error_text: &str,
    sequence_number: i64,
) -> Vec<u8> {
    let status = if status == 0 { 500 } else { status };
    let mut sequence_number = sequence_number.max(0);
    let mut message = error_text.trim().to_owned();
    if message.is_empty() {
        message = status_text(status).to_owned();
    }
    let mut code = error_class_for(status).code.to_owned();

    let trimmed = error_text.trim();
    if let Some(payload) = json_object(trimmed) {
        if let Some(value) = exact_i64(payload.get("sequence_number")) {
            sequence_number = value;
        }
    }
    if code.trim().is_empty() {
        code = "unknown_error".to_owned();
    }

    let error = error_detail(status, error_text, &code, message);
    match serde_json::to_vec(&OpenAiResponsesStreamErrorChunk {
        chunk_type: "error",
        error,
        sequence_number,
    }) {
        Ok(data) => data,
        Err(_) => fallback_error_chunk(&message_or_status(error_text, status), sequence_number),
    }
}

pub fn build_openai_responses_stream_failed_chunk(
    status: u16,
    error_text: &str,
    sequence_number: i64,
) -> Vec<u8> {
    let status = if status == 0 { 500 } else { status };
    let sequence_number = sequence_number.max(0);
    let error_chunk =
        build_openai_responses_stream_error_chunk(status, error_text, sequence_number);
    let parsed = serde_json::from_slice::<Value>(&error_chunk).ok();
    let sequence_number = parsed
        .as_ref()
        .and_then(|value| exact_i64(value.get("sequence_number")))
        .unwrap_or(0);
    let error = parsed
        .as_ref()
        .and_then(|value| value.get("error"))
        .filter(|value| !value.is_null())
        .cloned()
        .unwrap_or_else(|| {
            let code = error_class_for(status).code;
            let message = message_or_status(error_text, status);
            error_detail(status, error_text, code, message)
        });
    serde_json::to_vec(&OpenAiResponsesStreamFailedChunk {
        chunk_type: "response.failed",
        sequence_number,
        response: OpenAiResponsesStreamFailedResponse {
            status: "failed",
            error,
        },
    })
    .unwrap_or_else(|_| {
        br#"{"type":"response.failed","sequence_number":0,"response":{"status":"failed","error":{"type":"server_error","code":"internal_server_error","message":"internal error"}}}"#.to_vec()
    })
}

pub fn build_openai_responses_stream_error_event(
    status: u16,
    error_text: &str,
    sequence_number: i64,
) -> Vec<u8> {
    let payload = build_openai_responses_stream_error_chunk(status, error_text, sequence_number);
    let mut event = Vec::with_capacity(payload.len() + 8);
    event.extend_from_slice(b"data: ");
    event.extend_from_slice(&payload);
    event.extend_from_slice(b"\n\n");
    event
}

fn error_detail(status: u16, error_text: &str, code: &str, mut message: String) -> Value {
    let mut code = code.to_owned();
    let payload = json_object(error_text.trim());
    if let Some(payload) = payload.as_ref() {
        if let Some(error) = payload.get("error").and_then(Value::as_object) {
            return Value::Object(error.clone());
        }
        if let Some(response) = payload.get("response").and_then(Value::as_object) {
            if let Some(error) = response.get("error").and_then(Value::as_object) {
                return Value::Object(error.clone());
            }
        }
        if let Some(value) = non_empty_string(payload.get("message")) {
            message = value.to_owned();
        }
        if let Some(value) = payload.get("code").filter(|value| !value.is_null()) {
            code = sprint_json(value);
        }
    }

    let mut detail = Map::new();
    detail.insert(
        "type".to_owned(),
        Value::String(error_class_for(status).error_type.to_owned()),
    );
    detail.insert("code".to_owned(), Value::String(code));
    detail.insert("message".to_owned(), Value::String(message));
    detail.insert("param".to_owned(), Value::Null);
    if let Some(payload) = payload.as_ref() {
        if let Some(value) = non_empty_string(payload.get("type")) {
            if value != "error" {
                detail.insert("type".to_owned(), Value::String(value.to_owned()));
            }
        }
        if let Some(param) = payload.get("param") {
            detail.insert("param".to_owned(), param.clone());
        }
    }
    Value::Object(detail)
}

fn fallback_error_chunk(message: &str, sequence_number: i64) -> Vec<u8> {
    let detail = serde_json::json!({
        "type": "server_error",
        "code": "internal_server_error",
        "message": message,
        "param": null,
    });
    serde_json::to_vec(&OpenAiResponsesStreamErrorChunk {
        chunk_type: "error",
        error: detail,
        sequence_number,
    })
    .unwrap_or_else(|_| {
        br#"{"type":"error","error":{"type":"server_error","code":"internal_server_error","message":"internal error","param":null},"sequence_number":0}"#.to_vec()
    })
}

fn message_or_status(error_text: &str, status: u16) -> String {
    let message = error_text.trim();
    if message.is_empty() {
        status_text(status).to_owned()
    } else {
        message.to_owned()
    }
}

fn json_object(text: &str) -> Option<Map<String, Value>> {
    if text.is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(object)) => Some(object),
        _ => None,
    }
}

fn non_empty_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Go `fmt.Sprint` of a JSON string is the raw text, not a JSON string literal.
fn sprint_json(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn exact_i64(value: Option<&Value>) -> Option<i64> {
    let number = value?.as_number()?;
    number
        .as_i64()
        .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
}

fn status_text(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ if status >= 500 => "Internal Server Error",
        _ => "Unknown Error",
    }
}

#[cfg(test)]
#[path = "openai_responses_stream_error_test.rs"]
mod openai_responses_stream_error_test;
