// ref: sdk/api/handlers/openai/openai_responses_handlers.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only
//
// Focused port of `responsesSSEFramer.repairErrorPayload`. Private-event
// filtering and completed-payload repair stay unported.

use std::collections::BTreeMap;

use serde_json::Value;

use super::openai_responses_stream_error::{
    build_openai_responses_stream_error_chunk, build_openai_responses_stream_failed_chunk,
};

/// Codex Responses clients are a Codex user agent or an official Originator.
#[must_use]
pub fn is_codex_responses_client(headers: &BTreeMap<String, Vec<String>>) -> bool {
    if header_value(headers, "User-Agent")
        .is_some_and(|value| value.to_ascii_lowercase().contains("codex"))
    {
        return true;
    }
    let Some(originator) = header_value(headers, "Originator") else {
        return false;
    };
    let originator = originator.trim().to_ascii_lowercase();
    matches!(
        originator.as_str(),
        "codex desktop" | "codex-tui" | "codex_cli_rs"
    ) || originator.starts_with("codex desktop/")
        || originator.starts_with("codex-tui/")
        || originator.starts_with("codex_cli_rs/")
}

/// Rebuilds an error frame with the nested Responses stream-error chunk.
///
/// A Codex client receives `event: response.failed`. Every other client
/// receives `event: error`. Frames that are not error payloads pass through.
#[must_use]
pub fn repair_responses_sse_frame(frame: &[u8], codex_client: bool) -> Vec<u8> {
    let Some((event_name, payload)) = split_sse_data(frame) else {
        return frame.to_vec();
    };
    if payload == b"[DONE]" || serde_json::from_slice::<Value>(&payload).is_err() {
        return frame.to_vec();
    }
    let payload_type = serde_json::from_slice::<Value>(&payload)
        .ok()
        .and_then(|value| value.get("type").and_then(Value::as_str).map(str::to_owned));
    let event_is_error = event_name.as_deref().is_some_and(is_error_event_name)
        || payload_type.as_deref().is_some_and(is_error_event_name)
        || payload_has_error(&payload);
    if !event_is_error {
        return frame.to_vec();
    }
    let status = status_from_payload(&payload);
    let error_text = String::from_utf8_lossy(&payload);
    let sequence = sequence_number(&payload);
    let (event, chunk) = if codex_client {
        (
            "response.failed",
            build_openai_responses_stream_failed_chunk(status, &error_text, sequence),
        )
    } else {
        (
            "error",
            build_openai_responses_stream_error_chunk(status, &error_text, sequence),
        )
    };
    let mut frame = Vec::with_capacity(chunk.len() + event.len() + 16);
    frame.extend_from_slice(b"event: ");
    frame.extend_from_slice(event.as_bytes());
    frame.extend_from_slice(b"\ndata: ");
    frame.extend_from_slice(&chunk);
    frame.extend_from_slice(b"\n\n");
    frame
}

/// Transport failure with no upstream payload. Status defaults to 502.
#[must_use]
pub fn responses_transport_failure_frame(message: &str, codex_client: bool) -> Vec<u8> {
    let payload = format!(
        r#"{{"message":{}}}"#,
        serde_json::to_string(message).unwrap_or_default()
    );
    repair_responses_sse_frame(
        format!("event: error\ndata: {payload}\n\n").as_bytes(),
        codex_client,
    )
}

fn header_value<'a>(headers: &'a BTreeMap<String, Vec<String>>, name: &str) -> Option<&'a str> {
    headers.iter().find_map(|(key, values)| {
        if !key.eq_ignore_ascii_case(name) {
            return None;
        }
        values.iter().find_map(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
    })
}

fn is_error_event_name(name: &str) -> bool {
    matches!(name, "response.failed" | "response.error" | "error")
}

fn payload_has_error(payload: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return false;
    };
    if value.get("error").is_some_and(|error| !error.is_null()) {
        return true;
    }
    if value
        .pointer("/response/error")
        .is_some_and(|error| !error.is_null())
    {
        return true;
    }
    value.get("code").is_some() && value.get("message").is_some()
}

fn status_from_payload(payload: &[u8]) -> u16 {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return 502;
    };
    for pointer in [
        "/status",
        "/status_code",
        "/error/status",
        "/error/status_code",
        "/response/error/status",
        "/response/error/status_code",
    ] {
        let Some(candidate) = value.pointer(pointer).and_then(Value::as_u64) else {
            continue;
        };
        if (400..=599).contains(&candidate) {
            return u16::try_from(candidate).unwrap_or(502);
        }
    }
    502
}

fn sequence_number(payload: &[u8]) -> i64 {
    serde_json::from_slice::<Value>(payload)
        .ok()
        .and_then(|value| value.get("sequence_number").and_then(Value::as_i64))
        .unwrap_or(0)
}

fn split_sse_data(frame: &[u8]) -> Option<(Option<String>, Vec<u8>)> {
    let text = std::str::from_utf8(frame).ok()?;
    let mut event_name = None;
    let mut data = String::new();
    let mut saw_data = false;
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("data:") {
            saw_data = true;
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    saw_data.then_some((event_name, data.into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_timeout_uses_nested_server_error_for_non_codex_clients() {
        let frame = repair_responses_sse_frame(
            br#"event: response.failed
data: {"status":408,"message":"stream disconnected before completion"}

"#,
            false,
        );
        let text = String::from_utf8(frame).unwrap();
        assert!(text.starts_with("event: error\n"));
        assert!(text.contains(r#""code":"request_timeout""#));
        assert!(text.contains(r#""type":"server_error""#));
        assert!(!text.contains("response.failed"));
    }

    #[test]
    fn codex_client_timeout_is_response_failed() {
        let headers =
            BTreeMap::from([("Originator".to_owned(), vec!["codex_cli_rs/0.1".to_owned()])]);
        assert!(is_codex_responses_client(&headers));
        let frame = repair_responses_sse_frame(
            br#"data: {"status":408,"message":"stream disconnected before completion"}

"#,
            true,
        );
        let text = String::from_utf8(frame).unwrap();
        assert!(text.starts_with("event: response.failed\n"));
        assert!(text.contains(r#""code":"request_timeout""#));
        assert!(text.contains(r#""type":"server_error""#));
    }

    #[test]
    fn completed_frames_pass_through() {
        let frame = b"event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n";
        assert_eq!(repair_responses_sse_frame(frame, false), frame);
    }
}
