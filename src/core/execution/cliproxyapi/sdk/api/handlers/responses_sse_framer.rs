// ref: sdk/api/handlers/openai/openai_responses_handlers.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only
//
// Port of `responsesSSEFramer`: split-frame buffering, private-event
// filtering, output-item capture, and empty `response.output` repair.
// Handler redaction still runs before these frames. The full
// `responsesStreamErrorText` sanitizer is not reimplemented here.

use std::collections::BTreeMap;

use gjson::Kind;
use serde_json::Value;

use super::openai_responses_stream_error::{
    build_openai_responses_stream_error_chunk, build_openai_responses_stream_failed_chunk,
};

/// Codex Responses clients use an official Codex user agent or Originator.
///
/// Matches `IsCodexClientUserAgent` plus the Originator allow-list. A UA that
/// merely contains "codex", such as `codex_vscode`, is not an official client.
#[must_use]
pub fn is_codex_responses_client(headers: &BTreeMap<String, Vec<String>>) -> bool {
    if header_value(headers, "User-Agent").is_some_and(is_codex_client_user_agent) {
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

/// Stateful Responses SSE framer. One instance follows a whole stream.
#[derive(Debug)]
pub struct ResponsesSseFramer {
    pending: Vec<u8>,
    output_items: BTreeMap<i64, Vec<u8>>,
    unindexed_output_items: Vec<Vec<u8>>,
    last_event: String,
    terminal_event: String,
    failure_event: String,
    is_codex_client: bool,
    data_frames: i64,
}

impl ResponsesSseFramer {
    #[must_use]
    pub fn new(codex_client: bool) -> Self {
        Self {
            pending: Vec::new(),
            output_items: BTreeMap::new(),
            unindexed_output_items: Vec::new(),
            last_event: String::new(),
            terminal_event: String::new(),
            failure_event: if codex_client {
                "response.failed".to_owned()
            } else {
                "error".to_owned()
            },
            is_codex_client: codex_client,
            data_frames: 0,
        }
    }

    #[must_use]
    pub fn write_chunk(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        if chunk.is_empty() || !self.terminal_event.is_empty() {
            return out;
        }
        if starts_new_data_frame(&self.pending, chunk) {
            let pending = std::mem::take(&mut self.pending);
            self.write_frame(&mut out, &pending);
            if !self.terminal_event.is_empty() {
                return out;
            }
        }
        if needs_line_break(&self.pending, chunk) {
            self.pending.push(b'\n');
        }
        self.pending.extend_from_slice(chunk);
        loop {
            let frame_len = frame_len(&self.pending);
            if frame_len == 0 {
                break;
            }
            let frame: Vec<u8> = self.pending.drain(..frame_len).collect();
            self.write_frame(&mut out, &frame);
            if !self.terminal_event.is_empty() {
                self.pending.clear();
                return out;
            }
        }
        if trim_space(&self.pending).is_empty() {
            self.pending.clear();
            return out;
        }
        if self.pending.is_empty() || !can_emit_without_delimiter(&self.pending) {
            return out;
        }
        let frame = std::mem::take(&mut self.pending);
        self.write_frame(&mut out, &frame);
        out
    }

    #[must_use]
    pub fn flush(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.pending.is_empty() || !self.terminal_event.is_empty() {
            return out;
        }
        if trim_space(&self.pending).is_empty() {
            self.pending.clear();
            return out;
        }
        if !can_flush_without_delimiter(&self.pending) {
            self.pending.clear();
            return out;
        }
        let frame = std::mem::take(&mut self.pending);
        self.write_frame(&mut out, &frame);
        out
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        !self.terminal_event.is_empty()
    }

    #[must_use]
    pub fn is_failure(&self) -> bool {
        matches!(
            self.terminal_event.as_str(),
            "response.failed" | "response.error" | "error"
        )
    }

    #[must_use]
    pub fn data_frames(&self) -> i64 {
        self.data_frames
    }

    #[must_use]
    pub fn terminal_event(&self) -> &str {
        &self.terminal_event
    }

    #[must_use]
    pub fn last_event(&self) -> &str {
        &self.last_event
    }

    fn write_frame(&mut self, out: &mut Vec<u8>, frame: &[u8]) {
        let Some(frame) = self.repair_frame(frame) else {
            return;
        };
        if frame.is_empty() {
            return;
        }
        out.extend_from_slice(&frame);
        if frame.ends_with(b"\n\n") || frame.ends_with(b"\r\n\r\n") {
            return;
        }
        if frame.ends_with(b"\r\n") {
            out.extend_from_slice(b"\r\n");
        } else if frame.ends_with(b"\n") {
            out.push(b'\n');
        } else {
            out.extend_from_slice(b"\n\n");
        }
    }

    fn should_filter_private_event(&self, stream_event: &str, payload_type: &str) -> bool {
        self.private_event_name(stream_event) || self.private_event_name(payload_type)
    }

    fn private_event_name(&self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || is_error_event_name(name) {
            return false;
        }
        if name.starts_with("responsesapi.") {
            return true;
        }
        if self.is_codex_client {
            return name == "codex.rate_limits";
        }
        name.starts_with("codex.")
    }

    fn repair_frame(&mut self, frame: &[u8]) -> Option<Vec<u8>> {
        let stream_event = event_name(frame);
        if !stream_event.is_empty() && self.should_filter_private_event(&stream_event, "") {
            return None;
        }
        let Some(payload) = data_payload(frame) else {
            return Some(frame.to_vec());
        };
        if payload.is_empty() {
            return Some(frame.to_vec());
        }
        if payload == b"[DONE]" {
            self.data_frames += 1;
            return Some(frame.to_vec());
        }
        if serde_json::from_slice::<Value>(trim_space(&payload)).is_err() {
            return Some(frame.to_vec());
        }
        let payload_type = payload_type(&payload);
        if self.should_filter_private_event(&stream_event, &payload_type) {
            return None;
        }
        self.data_frames += 1;
        if is_error_event_name(&payload_type) || payload_has_error(&payload) {
            if !payload_type.is_empty() {
                self.last_event = payload_type;
            }
            return Some(self.repair_error_payload(&payload));
        }
        let mut event_type = payload_type.clone();
        if is_terminal_event_name(&stream_event) {
            event_type = stream_event;
        } else if event_type.is_empty() {
            event_type = event_name(frame);
        }
        if !event_type.is_empty() {
            self.last_event = event_type.clone();
        }
        if is_error_event_name(&event_type) {
            return Some(self.repair_error_payload(&payload));
        }
        if is_terminal_event_name(&event_type) {
            self.terminal_event = event_type.clone();
        }
        match event_type.as_str() {
            "response.output_item.done" => self.record_output_item(&payload),
            "response.completed" => {
                let repaired = self.repair_completed_payload(&payload);
                if repaired != payload {
                    return Some(frame_with_data(frame, &repaired));
                }
            }
            _ => {}
        }
        Some(frame.to_vec())
    }

    fn repair_error_payload(&mut self, payload: &[u8]) -> Vec<u8> {
        let status = status_from_payload(payload);
        let failure_event = if self.failure_event == "response.failed" {
            "response.failed"
        } else {
            "error"
        };
        self.terminal_event = failure_event.to_owned();
        let error_text = String::from_utf8_lossy(payload);
        let sequence = if self.data_frames > 0 {
            self.data_frames - 1
        } else {
            0
        };
        let (event, chunk) = if failure_event == "response.failed" {
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

    fn record_output_item(&mut self, payload: &[u8]) {
        let Ok(document) = std::str::from_utf8(payload) else {
            return;
        };
        let parsed = gjson::parse(document);
        let item = parsed.get("item");
        if !item.exists() || item.kind() != Kind::Object || item.get("type").str().is_empty() {
            return;
        }
        let raw = item.json();
        if raw.as_bytes().is_empty() {
            return;
        }
        let item_raw = raw.as_bytes().to_vec();
        let output_index = parsed.get("output_index");
        if output_index.exists() {
            self.output_items.insert(output_index.i64(), item_raw);
            return;
        }
        self.unindexed_output_items.push(item_raw);
    }

    fn repair_completed_payload(&self, payload: &[u8]) -> Vec<u8> {
        if self.output_items.is_empty() && self.unindexed_output_items.is_empty() {
            return payload.to_vec();
        }
        let Ok(document) = std::str::from_utf8(payload) else {
            return payload.to_vec();
        };
        let parsed = gjson::parse(document);
        let output = parsed.get("response.output");
        if output.exists() && (output.kind() != Kind::Array || gjson_array_nonempty(&output)) {
            return payload.to_vec();
        }
        let replacement = joined_output_items(&self.output_items, &self.unindexed_output_items);
        set_raw_response_output(payload, &replacement).unwrap_or_else(|| payload.to_vec())
    }
}

/// Rebuilds one already-delimited frame. Stateful streams should keep a
/// [`ResponsesSseFramer`] so earlier output items and data-frame counts survive.
#[must_use]
pub fn repair_responses_sse_frame(frame: &[u8], codex_client: bool) -> Vec<u8> {
    let mut framer = ResponsesSseFramer::new(codex_client);
    let mut emitted = framer.write_chunk(frame);
    emitted.extend(framer.flush());
    emitted
}

/// Transport failure with no upstream payload. Status defaults to 502.
///
/// `sequence` is the number of data frames already forwarded, matching
/// `forwardResponsesStream`'s terminal-error sequence.
#[must_use]
pub fn responses_transport_failure_frame(
    message: &str,
    codex_client: bool,
    sequence: i64,
) -> Vec<u8> {
    let payload = format!(
        r#"{{"message":{},"sequence_number":{}}}"#,
        serde_json::to_string(message).unwrap_or_else(|_| "\"\"".to_owned()),
        sequence.max(0),
    );
    repair_responses_sse_frame(
        format!("event: error\ndata: {payload}\n\n").as_bytes(),
        codex_client,
    )
}

fn is_codex_client_user_agent(user_agent: &str) -> bool {
    let user_agent = user_agent.trim();
    user_agent.starts_with("Codex Desktop/")
        || user_agent.starts_with("codex-tui/")
        || user_agent == "codex_cli_rs"
        || user_agent.starts_with("codex_cli_rs/")
        || user_agent.starts_with("codex_exec/")
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

fn is_terminal_event_name(name: &str) -> bool {
    matches!(
        name,
        "response.completed"
            | "response.incomplete"
            | "response.failed"
            | "response.done"
            | "response.error"
            | "error"
    )
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

fn payload_type(payload: &[u8]) -> String {
    let Ok(document) = std::str::from_utf8(payload) else {
        return String::new();
    };
    gjson::parse(document).get("type").str().to_owned()
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

fn gjson_array_nonempty(value: &gjson::Value) -> bool {
    let mut found = false;
    value.each(|_, _| {
        found = true;
        false
    });
    found
}

fn joined_output_items(indexed: &BTreeMap<i64, Vec<u8>>, unindexed: &[Vec<u8>]) -> Vec<u8> {
    let mut output = Vec::new();
    output.push(b'[');
    let mut written = 0;
    for item in indexed.values().chain(unindexed.iter()) {
        if written > 0 {
            output.push(b',');
        }
        output.extend_from_slice(item);
        written += 1;
    }
    output.push(b']');
    output
}

fn set_raw_response_output(payload: &[u8], replacement: &[u8]) -> Option<Vec<u8>> {
    let document = std::str::from_utf8(payload).ok()?;
    let existing = gjson::get(document, "response.output");
    if existing.exists() {
        return splice_gjson_value(payload, document, &existing, replacement);
    }
    let response = gjson::get(document, "response");
    if response.exists() && response.kind() == Kind::Object {
        let object = response.json();
        let updated = insert_json_property(object.as_bytes(), "output", replacement)?;
        return splice_gjson_value(payload, document, &response, &updated);
    }
    if trim_space(payload).first() == Some(&b'{') {
        let value = wrapped_output_object(replacement);
        return insert_json_property(payload, "response", &value);
    }
    None
}

fn wrapped_output_object(replacement: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(replacement.len() + 12);
    value.extend_from_slice(b"{\"output\":");
    value.extend_from_slice(replacement);
    value.push(b'}');
    value
}

fn splice_gjson_value(
    payload: &[u8],
    document: &str,
    existing: &gjson::Value,
    replacement: &[u8],
) -> Option<Vec<u8>> {
    let raw = existing.json();
    if raw.is_empty() {
        return None;
    }
    let document_start = document.as_ptr() as usize;
    let raw_start = raw.as_ptr() as usize;
    if raw_start < document_start {
        return None;
    }
    let start = raw_start - document_start;
    if start > payload.len() || raw.len() > payload.len() - start {
        return None;
    }
    let mut output = Vec::with_capacity(payload.len() - raw.len() + replacement.len());
    output.extend_from_slice(&payload[..start]);
    output.extend_from_slice(replacement);
    output.extend_from_slice(&payload[start + raw.len()..]);
    Some(output)
}

fn insert_json_property(object: &[u8], key: &str, value: &[u8]) -> Option<Vec<u8>> {
    let object = trim_space(object);
    if object.first() != Some(&b'{') || object.last() != Some(&b'}') {
        return None;
    }
    let inner = &object[1..object.len() - 1];
    let empty = trim_space(inner).is_empty();
    let mut out = Vec::with_capacity(object.len() + key.len() + value.len() + 4);
    out.push(b'{');
    if !empty {
        out.extend_from_slice(inner);
        out.push(b',');
    }
    out.push(b'"');
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(b"\":");
    out.extend_from_slice(value);
    out.push(b'}');
    Some(out)
}

fn frame_len(chunk: &[u8]) -> usize {
    if chunk.is_empty() {
        return 0;
    }
    let lf = find_slice(chunk, b"\n\n");
    let crlf = find_slice(chunk, b"\r\n\r\n");
    match (lf, crlf) {
        (None, None) => 0,
        (None, Some(index)) => index + 4,
        (Some(index), None) => index + 2,
        (Some(lf_index), Some(crlf_index)) if lf_index < crlf_index => lf_index + 2,
        (_, Some(crlf_index)) => crlf_index + 4,
    }
}

fn find_slice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn needs_more_data(chunk: &[u8]) -> bool {
    let trimmed = trim_space(chunk);
    !trimmed.is_empty() && has_field(trimmed, b"event:") && !has_field(trimmed, b"data:")
}

fn has_field(chunk: &[u8], prefix: &[u8]) -> bool {
    let mut rest = chunk;
    while !rest.is_empty() {
        let (line, next) = split_line(rest);
        if trim_space(line).starts_with(prefix) {
            return true;
        }
        rest = next;
    }
    false
}

fn can_emit_without_delimiter(chunk: &[u8]) -> bool {
    let trimmed = trim_space(chunk);
    if trimmed.is_empty()
        || needs_more_data(trimmed)
        || !has_field(trimmed, b"event:")
        || !has_field(trimmed, b"data:")
    {
        return false;
    }
    data_lines_valid(trimmed)
}

fn can_flush_without_delimiter(chunk: &[u8]) -> bool {
    let trimmed = trim_space(chunk);
    !trimmed.is_empty() && has_field(trimmed, b"data:") && data_lines_valid(trimmed)
}

fn starts_new_data_frame(pending: &[u8], chunk: &[u8]) -> bool {
    let trimmed_pending = trim_space(pending);
    if trimmed_pending.is_empty()
        || has_field(trimmed_pending, b"event:")
        || !has_field(trimmed_pending, b"data:")
        || !data_lines_valid(trimmed_pending)
    {
        return false;
    }
    trim_left(chunk, b" \t\r\n").starts_with(b"data:")
}

fn event_name(frame: &[u8]) -> String {
    let mut rest = frame;
    while !rest.is_empty() {
        let (line, next) = split_line(rest);
        let trimmed = trim_space(line);
        if let Some(name) = trimmed.strip_prefix(b"event:") {
            return String::from_utf8_lossy(trim_space(name)).into_owned();
        }
        rest = next;
    }
    String::new()
}

fn data_payload(frame: &[u8]) -> Option<Vec<u8>> {
    let mut payload = Vec::new();
    let mut found = false;
    let mut rest = frame;
    while !rest.is_empty() {
        let (line, next) = split_line(rest);
        let trimmed = trim_space(trim_right(line, b"\r"));
        if let Some(data) = trimmed.strip_prefix(b"data:") {
            if found {
                payload.push(b'\n');
            }
            payload.extend_from_slice(trim_space(data));
            found = true;
        }
        rest = next;
    }
    found.then_some(payload)
}

fn data_lines_valid(chunk: &[u8]) -> bool {
    let Some(payload) = data_payload(chunk) else {
        return true;
    };
    let payload = trim_space(&payload);
    payload.is_empty() || payload == b"[DONE]" || serde_json::from_slice::<Value>(payload).is_ok()
}

fn needs_line_break(pending: &[u8], chunk: &[u8]) -> bool {
    if pending.is_empty() || chunk.is_empty() {
        return false;
    }
    if pending.ends_with(b"\n") || pending.ends_with(b"\r") {
        return false;
    }
    if chunk[0] == b'\n' || chunk[0] == b'\r' {
        return false;
    }
    let trimmed = trim_left(chunk, b" \t");
    if trimmed.is_empty() {
        return false;
    }
    trimmed.starts_with(b"data:")
        || trimmed.starts_with(b"event:")
        || trimmed.starts_with(b"id:")
        || trimmed.starts_with(b"retry:")
        || trimmed.starts_with(b":")
}

fn frame_with_data(frame: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = frame;
    while !rest.is_empty() {
        let (line, next) = split_line(rest);
        let line = trim_right(line, b"\r");
        let trimmed = trim_space(line);
        if !trimmed.is_empty() && !trimmed.starts_with(b"data:") {
            out.extend_from_slice(line);
            out.push(b'\n');
        }
        rest = next;
    }
    let mut payload_rest = payload;
    if payload_rest.is_empty() {
        out.extend_from_slice(b"data: ");
        out.push(b'\n');
    }
    while !payload_rest.is_empty() {
        let (line, next) = split_line(payload_rest);
        out.extend_from_slice(b"data: ");
        out.extend_from_slice(line);
        out.push(b'\n');
        if next.is_empty() && payload_rest.ends_with(b"\n") {
            out.extend_from_slice(b"data: ");
            out.push(b'\n');
        }
        payload_rest = next;
    }
    out.push(b'\n');
    out
}

fn split_line(bytes: &[u8]) -> (&[u8], &[u8]) {
    if let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
        (&bytes[..index], &bytes[index + 1..])
    } else {
        (bytes, &[])
    }
}

fn trim_space(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !is_ascii_ws(*byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !is_ascii_ws(*byte))
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}

fn trim_left<'a>(bytes: &'a [u8], cutset: &[u8]) -> &'a [u8] {
    let start = bytes
        .iter()
        .position(|byte| !cutset.contains(byte))
        .unwrap_or(bytes.len());
    &bytes[start..]
}

fn trim_right<'a>(bytes: &'a [u8], cutset: &[u8]) -> &'a [u8] {
    let end = bytes
        .iter()
        .rposition(|byte| !cutset.contains(byte))
        .map_or(0, |index| index + 1);
    &bytes[..end]
}

fn is_ascii_ws(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn request_timeout_uses_nested_server_error_for_non_codex_clients() {
        let frame = repair_responses_sse_frame(
            br#"event: response.failed
data: {"status":408,"message":"stream disconnected before completion"}

"#,
            false,
        );
        let text = text(&frame);
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
            br#"event: response.failed
data: {"status":408,"message":"stream disconnected before completion"}

"#,
            true,
        );
        let text = text(&frame);
        assert!(text.starts_with("event: response.failed\n"));
        assert!(text.contains(r#""code":"request_timeout""#));
        assert!(text.contains(r#""type":"server_error""#));
    }

    #[test]
    fn official_codex_user_agent_is_narrower_than_a_codex_substring() {
        let codex_desktop = BTreeMap::from([(
            "User-Agent".to_owned(),
            vec!["Codex Desktop/1.0".to_owned()],
        )]);
        let codex_exec =
            BTreeMap::from([("User-Agent".to_owned(), vec!["codex_exec/0.1".to_owned()])]);
        let vscode = BTreeMap::from([(
            "User-Agent".to_owned(),
            vec!["codex_vscode/0.153.4 (Ubuntu 22.4.0; x86_64)".to_owned()],
        )]);
        assert!(is_codex_responses_client(&codex_desktop));
        assert!(is_codex_responses_client(&codex_exec));
        assert!(!is_codex_responses_client(&vscode));
    }

    #[test]
    fn completed_frames_pass_through() {
        let frame = b"event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n";
        assert_eq!(repair_responses_sse_frame(frame, false), frame);
    }

    #[test]
    fn waits_for_event_field_after_data() {
        let mut framer = ResponsesSseFramer::new(false);
        let first =
            framer.write_chunk(br#"data: {"response":{"id":"resp-1","status":"completed"}}"#);
        assert!(first.is_empty());
        let second = framer.write_chunk(b"event: response.completed");
        assert_eq!(framer.terminal_event(), "response.completed");
        assert_eq!(framer.last_event(), "response.completed");
        let got = text(&second);
        assert!(got.contains("data: "));
        assert!(got.contains("event: response.completed"));
    }

    #[test]
    fn flushes_multiline_data_without_delimiter() {
        let mut framer = ResponsesSseFramer::new(false);
        let chunk = b"event: response.completed\ndata: {\"type\":\"response.completed\",\ndata: \"response\":{\"id\":\"resp-1\",\"status\":\"completed\"}}";
        let mut output = framer.write_chunk(chunk);
        output.extend(framer.flush());
        assert_eq!(framer.terminal_event(), "response.completed");
        assert!(text(&output).contains("response.completed"));
    }

    #[test]
    fn payload_error_wins_over_completed_event_name() {
        let mut framer = ResponsesSseFramer::new(true);
        let output = framer.write_chunk(
            b"data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\"}}\nevent: response.completed\n\n",
        );
        let got = text(&output);
        assert_eq!(framer.terminal_event(), "response.failed");
        assert!(!got.contains("event: response.completed"));
        assert_eq!(got.matches("event: response.failed").count(), 1);
    }

    #[test]
    fn error_event_wins_over_payload_type() {
        let mut framer = ResponsesSseFramer::new(false);
        framer.write_chunk(
            b"event: error\ndata: {\"type\":\"provider.error\",\"message\":\"failed\"}\n\n",
        );
        assert_eq!(framer.terminal_event(), "error");

        let mut framer = ResponsesSseFramer::new(false);
        framer.write_chunk(b"data: {\"response\":{\"error\":{\"message\":\"failed\"}}}\n\n");
        assert_eq!(framer.terminal_event(), "error");
    }

    #[test]
    fn data_only_chunks_repair_empty_completed_output() {
        let mut framer = ResponsesSseFramer::new(false);
        let first = framer.write_chunk(
            br#"data: {"type":"response.output_item.done","item":{"type":"function_call","arguments":"{}"}}"#,
        );
        let second = framer.write_chunk(
            br#"data: {"type":"response.completed","response":{"id":"resp-1","output":[]}}"#,
        );
        let third = framer.flush();
        assert!(first.is_empty());
        let mut body = first;
        body.extend(second);
        body.extend(third);
        let got = text(&body);
        let parts: Vec<&str> = got.trim().split("\n\n").collect();
        assert_eq!(parts.len(), 2, "{got}");
        assert_eq!(
            parts[0],
            r#"data: {"type":"response.output_item.done","item":{"type":"function_call","arguments":"{}"}}"#
        );
        assert_eq!(
            parts[1],
            r#"data: {"type":"response.completed","response":{"id":"resp-1","output":[{"type":"function_call","arguments":"{}"}]}}"#
        );
    }

    #[test]
    fn indexed_output_items_are_sorted_before_unindexed_items() {
        let mut framer = ResponsesSseFramer::new(false);
        framer.write_chunk(
            b"data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"id\":\"fc-1\",\"name\":\"shell\"}}\n\n",
        );
        framer.write_chunk(
            b"data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg-0\"}}\n\n",
        );
        framer.write_chunk(
            b"data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\",\"id\":\"msg-unindexed\"}}\n\n",
        );
        let completed = framer.write_chunk(
            b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\",\"output\":[]}}\n\n",
        );
        let payload = data_payload(&completed).unwrap();
        let value: Value = serde_json::from_slice(&payload).unwrap();
        let output = value
            .pointer("/response/output")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(output.len(), 3);
        assert_eq!(output[0]["id"], "msg-0");
        assert_eq!(output[1]["name"], "shell");
        assert_eq!(output[2]["id"], "msg-unindexed");
    }

    #[test]
    fn non_codex_clients_drop_private_codex_and_timing_events() {
        let mut framer = ResponsesSseFramer::new(false);
        let mut output = Vec::new();
        output.extend(framer.write_chunk(b"event: codex.rate_limits\ndata: {\"type\":\"codex.rate_limits\",\"rate_limits\":{\"primary\":{\"used_percent\":42}}}\n\n"));
        output.extend(framer.write_chunk(b"event: codex.response.metadata\ndata: {\"type\":\"codex.response.metadata\",\"headers\":{\"x-turn-state\":\"turn-1\"}}\n\n"));
        output.extend(framer.write_chunk(b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-1\"}}\n\n"));
        output.extend(framer.write_chunk(b"event: responsesapi.websocket_timing\ndata: {\"type\":\"responsesapi.websocket_timing\",\"timing\":{\"duration_ms\":100}}\n\n"));
        output.extend(framer.write_chunk(b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\",\"status\":\"completed\"}}\n\n"));
        let got = text(&output);
        assert!(!got.contains("codex.rate_limits"));
        assert!(!got.contains("codex.response.metadata"));
        assert!(!got.contains("responsesapi.websocket_timing"));
        assert!(got.contains("event: response.created"));
        assert!(got.contains("event: response.completed"));
    }

    #[test]
    fn codex_client_keeps_metadata_and_drops_rate_limits_and_timing() {
        let mut framer = ResponsesSseFramer::new(true);
        let mut output = Vec::new();
        output.extend(
            framer.write_chunk(
                b"event: codex.rate_limits\ndata: {\"type\":\"codex.rate_limits\"}\n\n",
            ),
        );
        output.extend(framer.write_chunk(b"event: codex.response.metadata\ndata: {\"type\":\"codex.response.metadata\",\"headers\":{\"x-turn-state\":\"turn-1\"}}\n\n"));
        output.extend(framer.write_chunk(b"event: responsesapi.websocket_timing\ndata: {\"type\":\"responsesapi.websocket_timing\"}\n\n"));
        output.extend(framer.write_chunk(b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-1\"}}\n\n"));
        output.extend(framer.write_chunk(b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\",\"status\":\"completed\"}}\n\n"));
        let got = text(&output);
        assert!(!got.contains("codex.rate_limits"));
        assert!(!got.contains("responsesapi.websocket_timing"));
        assert!(got.contains("codex.response.metadata"));
        assert!(got.contains("event: response.created"));
        assert!(got.contains("event: response.completed"));
    }

    #[test]
    fn data_only_private_events_non_json_and_event_only_frames_are_dropped() {
        let mut framer = ResponsesSseFramer::new(false);
        let mut output = Vec::new();
        output.extend(framer.write_chunk(b"data: {\"type\":\"codex.rate_limits\",\"rate_limits\":{\"primary\":{\"used_percent\":42}}}\n\n"));
        output.extend(framer.write_chunk(b"event: codex.rate_limits\ndata: not-json-data\n\n"));
        output.extend(framer.write_chunk(b"event: codex.response.metadata\n\n"));
        output.extend(framer.write_chunk(b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-1\"}}\n\n"));
        output.extend(framer.write_chunk(b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\",\"status\":\"completed\"}}\n\n"));
        let got = text(&output);
        assert!(!got.contains("codex.rate_limits"));
        assert!(!got.contains("not-json-data"));
        assert!(!got.contains("codex.response.metadata"));
        assert!(got.contains("event: response.created"));
        assert!(got.contains("event: response.completed"));
    }

    #[test]
    fn error_sequence_uses_prior_data_frames_when_payload_omits_it() {
        let mut first = ResponsesSseFramer::new(false);
        let out =
            first.write_chunk(b"event: error\ndata: {\"error\":{\"code\":\"bad_request\"}}\n\n");
        let payload: Value = serde_json::from_slice(&data_payload(&out).unwrap()).unwrap();
        assert_eq!(payload["sequence_number"], 0);

        let mut framer = ResponsesSseFramer::new(false);
        framer.write_chunk(b"event: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":0}\n\n");
        framer.write_chunk(b"event: response.in_progress\ndata: {\"type\":\"response.in_progress\",\"sequence_number\":1}\n\n");
        let out =
            framer.write_chunk(b"event: error\ndata: {\"error\":{\"code\":\"cyber_policy\"}}\n\n");
        let payload: Value = serde_json::from_slice(&data_payload(&out).unwrap()).unwrap();
        assert_eq!(payload["sequence_number"], 2);
        assert_eq!(payload["error"]["code"], "cyber_policy");
    }

    #[test]
    fn split_frame_then_error_keeps_the_reassembled_event_and_sequence() {
        let mut framer = ResponsesSseFramer::new(false);
        assert!(framer.write_chunk(b"event: response.created").is_empty());
        let created = framer
            .write_chunk(b"\ndata: {\"type\":\"response.created\",\"sequence_number\":0}\n\n");
        assert!(text(&created).contains("event: response.created"));
        assert!(text(&created).contains("response.created"));
        let error = framer.write_chunk(b"event: error\ndata: {\"error\":{\"type\":\"invalid_request\",\"code\":\"cyber_policy\",\"param\":null}}\n\n");
        let payload: Value = serde_json::from_slice(&data_payload(&error).unwrap()).unwrap();
        assert!(text(&error).starts_with("event: error\n"));
        assert_eq!(payload["sequence_number"], 1);
        assert_eq!(payload["error"]["code"], "cyber_policy");
        assert!(payload["error"]["param"].is_null());
    }

    #[test]
    fn flush_drops_incomplete_trailing_data() {
        let mut framer = ResponsesSseFramer::new(false);
        assert!(framer
            .write_chunk(b"data: {\"type\":\"response.created\"")
            .is_empty());
        assert!(framer.flush().is_empty());
        assert!(!framer.is_terminal());
    }

    #[test]
    fn newline_prefixed_chunks_do_not_gain_an_extra_break() {
        assert!(!needs_line_break(b"event: response.created", b"\n"));
        assert!(!needs_line_break(b"event: response.created", b"\r\n"));
    }
}
