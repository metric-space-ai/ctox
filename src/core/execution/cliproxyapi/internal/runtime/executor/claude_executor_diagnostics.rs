// ref: internal/runtime/executor/claude_executor_diagnostics.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use super::helps::{
    begin_claude_diagnostics, commit_claude_diagnostics as commit_state, pin_claude_session_date,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaudeDiagnosticsRequestState {
    key: String,
    sequence: u64,
    previous_message_id: String,
}

impl ClaudeDiagnosticsRequestState {
    pub fn pin_date(&self, date: &str) -> String {
        pin_claude_session_date(&self.key, date)
    }
}

/// Resolves continuity before cloaking so its date can be pinned. Injection
/// remains after cloaking to preserve the observable diagnostics member order.
pub fn begin_claude_diagnostics_request(
    credential_identity: &str,
    session_id: &str,
) -> ClaudeDiagnosticsRequestState {
    let (key, sequence, previous_message_id) =
        begin_claude_diagnostics(credential_identity, session_id);
    ClaudeDiagnosticsRequestState {
        key,
        sequence,
        previous_message_id,
    }
}

/// Adds the continuity object immediately after `context_management`, matching
/// Claude Code's observable member order. The cache key retains only a digest
/// of the credential identity and session.
pub fn inject_claude_diagnostics(
    body: &[u8],
    credential_identity: &str,
    session_id: &str,
) -> (Vec<u8>, ClaudeDiagnosticsRequestState) {
    inject_claude_diagnostics_with_state(
        body,
        begin_claude_diagnostics_request(credential_identity, session_id),
    )
}

pub fn inject_claude_diagnostics_with_state(
    body: &[u8],
    state: ClaudeDiagnosticsRequestState,
) -> (Vec<u8>, ClaudeDiagnosticsRequestState) {
    if state.key.is_empty() {
        return (body.to_vec(), ClaudeDiagnosticsRequestState::default());
    }
    let diagnostics = json!({
        "previous_message_id": if state.previous_message_id.is_empty() { Value::Null } else { Value::String(state.previous_message_id.clone()) }
    });
    let encoded = serde_json::to_vec(&diagnostics)
        .unwrap_or_else(|_| b"{\"previous_message_id\":null}".to_vec());
    if let Some((start, end)) = top_level_member_value_range(body, "diagnostics") {
        let mut output = Vec::with_capacity(body.len() - (end - start) + encoded.len());
        output.extend_from_slice(&body[..start]);
        output.extend_from_slice(&encoded);
        output.extend_from_slice(&body[end..]);
        return (output, state);
    }
    if let Some(end) = top_level_member_value_end(body, "context_management") {
        let mut output = Vec::with_capacity(body.len() + encoded.len() + 15);
        output.extend_from_slice(&body[..end]);
        output.extend_from_slice(b",\"diagnostics\":");
        output.extend_from_slice(&encoded);
        output.extend_from_slice(&body[end..]);
        return (output, state);
    }
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return (body.to_vec(), ClaudeDiagnosticsRequestState::default());
    };
    let Some(object) = root.as_object_mut() else {
        return (body.to_vec(), ClaudeDiagnosticsRequestState::default());
    };
    object.insert("diagnostics".to_owned(), diagnostics);
    (
        serde_json::to_vec(&root).unwrap_or_else(|_| body.to_vec()),
        state,
    )
}

pub fn commit_claude_diagnostics(state: &ClaudeDiagnosticsRequestState, message_id: &str) {
    commit_state(&state.key, state.sequence, message_id);
}

pub fn claude_message_id_from_response(data: &[u8]) -> String {
    serde_json::from_slice::<Value>(data)
        .ok()
        .and_then(|root| {
            root.get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

pub fn observe_claude_stream_line(line: &[u8], message_id: &mut String, completed: &mut bool) {
    let line = trim_ascii(line);
    let Some(payload) = line.strip_prefix(b"data:") else {
        return;
    };
    let Ok(root) = serde_json::from_slice::<Value>(trim_ascii(payload)) else {
        return;
    };
    match root.get("type").and_then(Value::as_str) {
        Some("message_start") => {
            if let Some(id) = root
                .pointer("/message/id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
            {
                *message_id = id.to_owned();
            }
        }
        Some("message_stop") => *completed = true,
        _ => {}
    }
}

pub fn claude_message_id_from_sse(data: &[u8]) -> String {
    let mut id = String::new();
    let mut completed = false;
    for line in data.split(|byte| *byte == b'\n') {
        observe_claude_stream_line(line, &mut id, &mut completed);
    }
    if completed {
        id
    } else {
        String::new()
    }
}

fn top_level_member_value_end(body: &[u8], key: &str) -> Option<usize> {
    top_level_member_value_range(body, key).map(|(_, end)| end)
}

fn top_level_member_value_range(body: &[u8], key: &str) -> Option<(usize, usize)> {
    let needle = format!("\"{key}\"");
    let start = body
        .windows(needle.len())
        .position(|part| part == needle.as_bytes())?;
    let mut index = start + needle.len();
    while body.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    if body.get(index) != Some(&b':') {
        return None;
    }
    index += 1;
    while body.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    let mut depth = 0_u32;
    let mut string = false;
    let mut escaped = false;
    for (offset, byte) in body[index..].iter().copied().enumerate() {
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return Some((index, index + offset + 1));
                }
            }
            b',' | b'}' if depth == 0 => return Some((index, index + offset)),
            _ => {}
        }
    }
    None
}

fn trim_ascii(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}
