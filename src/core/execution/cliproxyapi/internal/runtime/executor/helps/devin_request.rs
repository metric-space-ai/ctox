// ref: internal/runtime/executor/helps/devin_wire.go:133-192,253-522,620-661,1031-1064
// ref: internal/translator/common/devin_tools.go:1-71
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::cloak_obfuscate::SensitiveWordMatcher;
use crate::internal::util::is_claude_code_attribution_system_text;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::fmt::Write;
use std::sync::Mutex;
use uuid::Uuid;

pub const DEVIN_DEFAULT_BASE_URL: &str = "https://server.codeium.com";
pub const DEVIN_CHAT_PATH: &str = "/exa.api_server_pb.ApiServerService/GetChatMessage";
pub const DEVIN_DEFAULT_CLIENT_NAME: &str = "chisel";
pub const DEVIN_DEFAULT_CLIENT_VERSION: &str = "3000.10.21";
pub const DEVIN_FINGERPRINT_HEX_LEN: usize = 732;
pub const DEVIN_DEFAULT_MAX_TOKENS: i64 = 128_000;
const MAX_SESSION_TURN_COUNTERS: usize = 5_000;

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DevinTool {
    pub name: String,
    pub description: String,
    pub parameters: Vec<u8>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DevinToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DevinImage {
    pub base64_data: String,
    pub mime_type: String,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DevinPrompt {
    pub message_id: String,
    pub source: i64,
    pub content: String,
    pub images: Vec<DevinImage>,
    pub tool_calls: Vec<DevinToolCall>,
    pub tool_call_id: String,
    pub original_tool_call_id: String,
    pub is_orphaned_tool: bool,
    pub thinking: String,
    pub signature: Vec<u8>,
    pub signature_type: String,
}

/// All runtime identity and configuration is explicit. Credentials and prompt
/// content deliberately have no Debug implementation.
pub struct DevinChatRequest<'a> {
    pub session_token: &'a str,
    pub device_seed: &'a str,
    pub chat_model_uid: &'a str,
    pub system_prompt: &'a str,
    pub prompts: &'a [DevinPrompt],
    pub tools: &'a [DevinTool],
    pub temperature: Option<f64>,
    pub max_tokens: i64,
    pub session_id: &'a str,
    pub cascade_id: &'a str,
    pub matcher: Option<&'a SensitiveWordMatcher>,
}

#[derive(Default)]
struct TurnCounters {
    values: HashMap<String, u64>,
    recency: VecDeque<String>,
}

/// Instance-owned equivalent of upstream's bounded per-session counter cache.
pub struct DevinSessionTurns {
    counters: Mutex<TurnCounters>,
    capacity: usize,
}

impl Default for DevinSessionTurns {
    fn default() -> Self {
        Self::with_capacity(MAX_SESSION_TURN_COUNTERS)
    }
}

impl DevinSessionTurns {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0 && capacity <= MAX_SESSION_TURN_COUNTERS);
        Self {
            counters: Mutex::new(TurnCounters::default()),
            capacity,
        }
    }

    pub fn next(&self, session_id: &str) -> i64 {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return 0;
        }
        let mut counters = self
            .counters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(index) = counters.recency.iter().position(|key| key == session_id) {
            counters.recency.remove(index);
        } else if counters.values.len() == self.capacity {
            if let Some(oldest) = counters.recency.pop_front() {
                counters.values.remove(&oldest);
            }
        }
        counters.recency.push_back(session_id.to_owned());
        let counter = counters.values.entry(session_id.to_owned()).or_default();
        let turn = *counter;
        *counter = counter.wrapping_add(1);
        turn as i64
    }

    pub fn reset(&self, session_id: &str) {
        let session_id = session_id.trim();
        let mut counters = self
            .counters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        counters.values.remove(session_id);
        if let Some(index) = counters.recency.iter().position(|key| key == session_id) {
            counters.recency.remove(index);
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{byte:02x}").expect("String write");
    }
    result
}

pub fn generate_devin_device_fingerprint(seed: &str) -> String {
    let mut random = [0_u8; DEVIN_FINGERPRINT_HEX_LEN / 2];
    if seed.is_empty() && getrandom::fill(&mut random).is_ok() {
        return hex(&random);
    }
    let fallback = seed.is_empty().then(|| Uuid::new_v4().to_string());
    let seed = fallback.as_deref().unwrap_or(seed);
    let mut result = String::with_capacity(DEVIN_FINGERPRINT_HEX_LEN + 64);
    for counter in 0..12 {
        result.push_str(&hex(&Sha256::digest(
            format!("{seed}-{counter}").as_bytes(),
        )));
    }
    result.truncate(DEVIN_FINGERPRINT_HEX_LEN);
    result
}

pub fn generate_devin_sentry_trace() -> String {
    let mut random = [0_u8; 24];
    if getrandom::fill(&mut random).is_ok() {
        return format!("{}-{}-1", hex(&random[..16]), hex(&random[16..]));
    }
    let trace = Uuid::new_v4().simple().to_string();
    let span = Uuid::new_v4().simple().to_string();
    format!("{trace}-{}-1", &span[..16])
}

fn append_varint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        bytes.push(value as u8 | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn append_tag(bytes: &mut Vec<u8>, field: u32, wire_type: u8) {
    append_varint(bytes, u64::from(field) << 3 | u64::from(wire_type));
}

fn append_number(bytes: &mut Vec<u8>, field: u32, value: u64) {
    append_tag(bytes, field, 0);
    append_varint(bytes, value);
}

fn append_data(bytes: &mut Vec<u8>, field: u32, value: &[u8]) {
    append_tag(bytes, field, 2);
    append_varint(bytes, value.len() as u64);
    bytes.extend_from_slice(value);
}

fn append_text(bytes: &mut Vec<u8>, field: u32, value: &str) {
    append_data(bytes, field, value.as_bytes());
}

fn append_float(bytes: &mut Vec<u8>, field: u32, value: f64) {
    append_tag(bytes, field, 1);
    bytes.extend_from_slice(&value.to_bits().to_le_bytes());
}

fn native_go_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

pub fn build_devin_client_metadata_bytes(
    session_token: &str,
    device_seed: &str,
    os_name: &str,
) -> Vec<u8> {
    let os_name = if os_name.is_empty() {
        native_go_os()
    } else {
        os_name
    };
    let fingerprint = generate_devin_device_fingerprint(device_seed);
    let mut bytes = Vec::new();
    for (field, text) in [
        (1, DEVIN_DEFAULT_CLIENT_NAME),
        (2, DEVIN_DEFAULT_CLIENT_VERSION),
        (3, session_token),
        (4, "en"),
        (5, os_name),
        (7, DEVIN_DEFAULT_CLIENT_VERSION),
        (12, DEVIN_DEFAULT_CLIENT_NAME),
        (31, &fingerprint),
    ] {
        append_text(&mut bytes, field, text);
    }
    bytes
}

pub fn is_devin_codex_app_automation_update(namespace: &str, name: &str) -> bool {
    (namespace.trim().eq_ignore_ascii_case("mcp__codex_app")
        && name.trim().eq_ignore_ascii_case("automation_update"))
        || name
            .trim()
            .eq_ignore_ascii_case("mcp__codex_app__automation_update")
}

fn replace_folded_phrase(text: &str, phrase: &str, replacement: &str) -> String {
    // Go's case-insensitive regexp uses Unicode simple folding. For an ASCII
    // pattern only long-s and Kelvin sign add folds beyond ASCII letter case.
    let expected: Vec<char> = phrase.chars().collect();
    let mut output = String::new();
    let mut position = 0;
    while position < text.len() {
        let mut end = position;
        let matched = expected.iter().all(|expected| {
            let Some(actual) = text[end..].chars().next() else {
                return false;
            };
            if actual.eq_ignore_ascii_case(expected)
                || (expected.eq_ignore_ascii_case(&'s') && actual == 'ſ')
                || (expected.eq_ignore_ascii_case(&'k') && actual == 'K')
            {
                end += actual.len_utf8();
                true
            } else {
                false
            }
        });
        if matched {
            output.push_str(replacement);
            position = end;
        } else {
            let actual = text[position..].chars().next().expect("nonempty text");
            output.push(actual);
            position += actual.len_utf8();
        }
    }
    output
}

pub fn sanitize_devin_tool_description(tool_name: &str, description: &str) -> String {
    let name = tool_name.trim().to_lowercase();
    let mut description = description.to_owned();
    if name == "exec_command" || name.ends_with("__exec_command") {
        let target = "returning output or a session ID for ongoing interaction";
        let replacement = "returning output or an session ID for ongoing interaction";
        if !description.contains(replacement) {
            description = if description.contains(target) {
                description.replace(target, replacement)
            } else {
                replace_folded_phrase(&description, target, replacement)
            };
        }
    }
    if name == "write_stdin" || name.ends_with("__write_stdin") {
        let target =
            "Writes characters to an existing unified exec session and returns recent output.";
        let replacement =
            "Writes characters to a existing unified exec session and returns recent output.";
        if !description.contains(replacement) {
            description = if description.contains(target) {
                description.replace(target, replacement)
            } else {
                replace_folded_phrase(
                    &description,
                    target.trim_end_matches('.'),
                    replacement.trim_end_matches('.'),
                )
            };
        }
    }
    description
}

pub fn sanitize_devin_system_prompt(
    prompt: &str,
    matcher: Option<&SensitiveWordMatcher>,
) -> String {
    let normalized = prompt.replace("\r\n", "\n");
    let mut kept = Vec::new();
    for line in normalized.split('\n') {
        let trimmed = line.trim();
        if is_claude_code_attribution_system_text(trimmed)
            || trimmed.starts_with("You are Claude Code")
            || trimmed.contains("authorized security testing")
            || trimmed.contains("destructive techniques, DoS attacks")
            || trimmed.contains("Claude Code is available as a CLI")
            || trimmed.contains("Fast mode for Claude Code")
            || trimmed.contains("Codex refers to the open-source agentic coding interface")
            || trimmed.contains(
                "- Don’t output ANSI escape codes directly — the CLI renderer applies them.",
            )
            || matcher.is_some_and(|matcher| matcher.obfuscate_text(trimmed) != trimmed)
        {
            continue;
        }
        kept.push(line);
    }
    let result = kept.join("\n").trim().to_owned();
    matcher
        .filter(|_| !result.is_empty())
        .map_or_else(|| result.clone(), |matcher| matcher.obfuscate_text(&result))
}

pub fn build_devin_get_chat_message_request(
    input: &DevinChatRequest<'_>,
    turns: &DevinSessionTurns,
) -> Vec<u8> {
    let max_tokens = if input.max_tokens <= 0 {
        DEVIN_DEFAULT_MAX_TOKENS
    } else {
        input.max_tokens
    };
    let session_id = if input.session_id.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        input.session_id.to_owned()
    };
    let cascade_id = if input.cascade_id.is_empty() {
        session_id.as_str()
    } else {
        input.cascade_id
    };
    let mut bytes = Vec::new();
    append_data(
        &mut bytes,
        1,
        &build_devin_client_metadata_bytes(input.session_token, input.device_seed, native_go_os()),
    );
    if !input.system_prompt.is_empty() {
        let system = sanitize_devin_system_prompt(input.system_prompt, input.matcher);
        if !system.is_empty() {
            append_text(&mut bytes, 2, &system);
        }
    }
    for prompt in input.prompts {
        let mut encoded = Vec::new();
        let message_id = if prompt.message_id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            prompt.message_id.clone()
        };
        append_text(&mut encoded, 1, &message_id);
        append_number(
            &mut encoded,
            2,
            if prompt.source <= 0 {
                1
            } else {
                prompt.source as u64
            },
        );
        append_text(&mut encoded, 3, &prompt.content);
        for tool in &prompt.tool_calls {
            let mut encoded_tool = Vec::new();
            for (field, value) in [
                (1, tool.id.as_str()),
                (2, tool.name.as_str()),
                (3, tool.arguments.as_str()),
            ] {
                if !value.is_empty() {
                    append_text(&mut encoded_tool, field, value);
                }
            }
            append_data(&mut encoded, 6, &encoded_tool);
        }
        if !prompt.tool_call_id.is_empty() {
            append_text(&mut encoded, 7, &prompt.tool_call_id);
        }
        for image in &prompt.images {
            let base64 = image.base64_data.trim();
            if base64.is_empty() {
                continue;
            }
            let mime_type = image.mime_type.trim();
            let mut encoded_image = Vec::new();
            append_text(&mut encoded_image, 1, base64);
            append_text(
                &mut encoded_image,
                2,
                if mime_type.is_empty() {
                    "image/png"
                } else {
                    mime_type
                },
            );
            append_data(&mut encoded, 10, &encoded_image);
        }
        if !prompt.thinking.is_empty() {
            append_text(&mut encoded, 11, &prompt.thinking);
        }
        if !prompt.signature.is_empty() {
            append_data(&mut encoded, 12, &prompt.signature);
        }
        if !prompt.signature_type.is_empty() {
            append_text(&mut encoded, 18, &prompt.signature_type);
        }
        append_data(&mut bytes, 3, &encoded);
    }
    append_number(&mut bytes, 7, 5);
    let mut completion = Vec::new();
    append_number(&mut completion, 1, 1);
    append_number(&mut completion, 2, max_tokens as u64);
    append_number(&mut completion, 3, 400);
    append_float(&mut completion, 5, input.temperature.unwrap_or(1.0));
    append_number(&mut completion, 7, 40);
    append_float(&mut completion, 8, f64::from(0.95_f32));
    append_data(&mut bytes, 8, &completion);
    for tool in input.tools {
        if tool.name.is_empty() || is_devin_codex_app_automation_update("", &tool.name) {
            continue;
        }
        let mut encoded_tool = Vec::new();
        append_text(&mut encoded_tool, 1, &tool.name);
        let description = tool.description.replace(
            "Takes a task_id parameter identifying the task",
            "Takes a taskId parameter identifying the task",
        );
        let description = sanitize_devin_tool_description(&tool.name, &description);
        if !description.is_empty() {
            append_text(&mut encoded_tool, 2, &description);
        }
        if !tool.parameters.is_empty() {
            append_data(&mut encoded_tool, 3, &tool.parameters);
        }
        append_data(&mut bytes, 10, &encoded_tool);
    }
    let turn = turns.next(&session_id);
    let mut session = Vec::new();
    append_text(&mut session, 1, &session_id);
    if turn > 0 {
        append_number(&mut session, 2, turn as u64);
    }
    append_number(&mut session, 3, 4);
    if input
        .prompts
        .last()
        .is_some_and(|prompt| prompt.source == 1)
        && (turn == 0
            || input.prompts.len() < 2
            || input.prompts[input.prompts.len() - 2].source != 1)
    {
        append_number(&mut session, 4, 14);
    }
    append_data(&mut bytes, 15, &session);
    append_text(&mut bytes, 16, cascade_id);
    append_number(&mut bytes, 20, 1);
    append_text(&mut bytes, 21, input.chat_model_uid);
    bytes
}

/// Returns the same complete wire-byte prefix as upstream's UTF8SplitBuffer.
/// Invalid bytes are retained, while at most three incomplete trailing bytes
/// wait for the next frame. Serialization, rather than this helper, owns replacement.
#[derive(Default, Clone)]
pub struct DevinUtf8SplitBuffer {
    remainder: Vec<u8>,
}

impl DevinUtf8SplitBuffer {
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut combined = std::mem::take(&mut self.remainder);
        combined.extend_from_slice(chunk);
        let mut complete = 0;
        while complete < combined.len() {
            match std::str::from_utf8(&combined[complete..]) {
                Ok(_) => {
                    complete = combined.len();
                }
                Err(error) => {
                    complete += error.valid_up_to();
                    match error.error_len() {
                        Some(length) => complete += length,
                        None => break,
                    }
                }
            }
        }
        self.remainder = combined.split_off(complete);
        combined
    }

    pub fn pending_bytes(&self) -> &[u8] {
        &self.remainder
    }
}
