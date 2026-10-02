// ref: internal/runtime/executor/claude_executor_cloaking.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

use chrono::{Datelike, Utc};
use chrono_tz::Tz;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::sdk::cliproxy::executor::{RequestScopedError, StatusError};

use super::helps::{
    build_sensitive_word_matcher, generate_fake_user_id, is_valid_user_id,
    obfuscate_sensitive_words,
};

const FINGERPRINT_SALT: &str = "59cf53e54c78";

/// Explicit, account-owned replacement for the Go executor's mixture of global
/// config, auth metadata, and request-context lookups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeCloakPolicy {
    pub mode: String,
    pub strict_mode: bool,
    pub sensitive_words: Vec<String>,
    pub client_user_agent: String,
    pub billing_version: String,
    pub entrypoint: String,
    pub workload: String,
    pub oauth_mode: bool,
    pub sign_cch: bool,
    /// A strongly verified Claude Code client is a passthrough client. Even an
    /// operator `always` policy must not replace its measured wire identity.
    pub verified_claude_code: bool,
    /// Host-resolved local date. The host owns timezone configuration; tests
    /// and embedders can provide the exact Claude credential-local date here.
    pub current_date: Option<String>,
    /// Typed credential-local timezone. CTOX validates this in durable runtime
    /// config and never consults process environment variables for it.
    pub timezone: Tz,
}

impl ClaudeCloakPolicy {
    pub fn oauth_default() -> Self {
        Self {
            mode: "auto".to_owned(),
            strict_mode: false,
            sensitive_words: Vec::new(),
            client_user_agent: String::new(),
            billing_version: "2.1.280".to_owned(),
            entrypoint: "cli".to_owned(),
            workload: String::new(),
            oauth_mode: true,
            sign_cch: true,
            verified_claude_code: false,
            current_date: None,
            timezone: chrono_tz::UTC,
        }
    }

    pub fn with_timezone(mut self, timezone: Tz) -> Self {
        self.timezone = timezone;
        self
    }

    /// Candidate wire policy: only a strongly confirmed Claude Code request
    /// may bypass cloaking in `auto` mode. A copied User-Agent is deliberately
    /// insufficient, and confirmed native clients remain passthrough even when
    /// an operator configured `always`.
    pub fn should_cloak_request(&self) -> bool {
        if self.verified_claude_code {
            return false;
        }
        !self.mode.trim().eq_ignore_ascii_case("never")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCallerSystemBlockError {
    index: usize,
    block_type: String,
}

impl ClaudeCallerSystemBlockError {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn block_type(&self) -> &str {
        &self.block_type
    }
}

impl fmt::Display for ClaudeCallerSystemBlockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid_request_error: system.{}.type: Input should be 'text'. System instructions support text only, but this block has type {:?}. Move non-text content into a user message.",
            self.index, self.block_type
        )
    }
}

impl std::error::Error for ClaudeCallerSystemBlockError {}

impl RequestScopedError for ClaudeCallerSystemBlockError {
    fn is_request_scoped(&self) -> bool {
        true
    }
}

impl StatusError for ClaudeCallerSystemBlockError {
    fn status_code(&self) -> u16 {
        400
    }
}

impl Default for ClaudeCloakPolicy {
    fn default() -> Self {
        Self::oauth_default()
    }
}

pub fn parse_claude_entrypoint(user_agent: &str) -> String {
    let Some(start) = user_agent.find('(') else {
        return "cli".to_owned();
    };
    let Some(end) = user_agent.rfind(')') else {
        return "cli".to_owned();
    };
    if end <= start {
        return "cli".to_owned();
    }
    user_agent[start + 1..end]
        .split(',')
        .nth(1)
        .map(str::trim)
        .filter(|entrypoint| !entrypoint.is_empty())
        .unwrap_or("cli")
        .to_owned()
}

pub fn compute_claude_fingerprint(message_text: &str, version: &str) -> String {
    let runes: Vec<char> = message_text.chars().collect();
    let fingerprint: String = [4_usize, 7, 20]
        .into_iter()
        .map(|index| runes.get(index).copied().unwrap_or('0'))
        .collect();
    let digest = Sha256::digest(format!("{FINGERPRINT_SALT}{fingerprint}{version}").as_bytes());
    format!("{digest:x}")[..3].to_owned()
}

pub fn generate_claude_billing_header(
    _payload: &[u8],
    cch_signing: bool,
    version: &str,
    message_text: &str,
    entrypoint: &str,
    workload: &str,
) -> String {
    let entrypoint = if entrypoint.trim().is_empty() {
        "cli"
    } else {
        entrypoint.trim()
    };
    let build = compute_claude_fingerprint(message_text, version);
    let workload = if workload.trim().is_empty() {
        String::new()
    } else {
        format!(" cc_workload={};", workload.trim())
    };
    let cch = if cch_signing { " cch=00000;" } else { "" };
    format!(
        "x-anthropic-billing-header: cc_version={version}.{build}; cc_entrypoint={entrypoint};{cch}{workload}"
    )
}

pub fn claude_billing_fingerprint_message_text(payload: &[u8]) -> String {
    let Ok(root) = serde_json::from_slice::<Value>(payload) else {
        return String::new();
    };
    root.get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .filter_map(|message| message.get("content"))
        .filter_map(message_content_text)
        .rfind(|text| !text.is_empty())
        .unwrap_or_default()
}

/// Billing fallback used when late CCH finalization cannot reuse an installed
/// block. It deliberately carries the `00000` placeholder for the signer.
pub fn claude_cch_fallback_billing_header(
    payload: &[u8],
    version: &str,
    entrypoint: &str,
    workload: &str,
) -> String {
    generate_claude_billing_header(
        payload,
        true,
        version,
        &claude_billing_fingerprint_message_text(payload),
        entrypoint,
        workload,
    )
}

pub fn apply_claude_cloaking(
    payload: &[u8],
    _model: &str,
    policy: &ClaudeCloakPolicy,
    cached_user_id: Option<&str>,
) -> Vec<u8> {
    if !policy.should_cloak_request() {
        return payload.to_vec();
    }
    let Ok(mut body) = try_inject_claude_system_instructions(payload, policy) else {
        // Preserve the caller's body rather than silently dropping an
        // unsupported system block. `try_apply_claude_cloaking` exposes the
        // request-scoped 400 to integrations that can return typed errors.
        return payload.to_vec();
    };
    body = inject_claude_code_context_management(&body);
    body = inject_fake_claude_user_id(&body, cached_user_id);
    let matcher = build_sensitive_word_matcher(&policy.sensitive_words);
    obfuscate_sensitive_words(&body, matcher.as_ref())
}

pub fn try_apply_claude_cloaking(
    payload: &[u8],
    model: &str,
    policy: &ClaudeCloakPolicy,
    cached_user_id: Option<&str>,
) -> Result<Vec<u8>, ClaudeCallerSystemBlockError> {
    if !policy.should_cloak_request() {
        return Ok(payload.to_vec());
    }
    let mut body = try_inject_claude_system_instructions(payload, policy)?;
    body = inject_claude_code_context_management(&body);
    body = inject_fake_claude_user_id(&body, cached_user_id);
    let matcher = build_sensitive_word_matcher(&policy.sensitive_words);
    let _ = model; // Candidate no longer special-cases Haiku cloaking.
    Ok(obfuscate_sensitive_words(&body, matcher.as_ref()))
}

pub fn inject_fake_claude_user_id(payload: &[u8], cached_user_id: Option<&str>) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    let Some(object) = root.as_object_mut() else {
        return payload.to_vec();
    };
    let existing = object
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("user_id"))
        .and_then(Value::as_str);
    if existing.is_some_and(is_valid_user_id) {
        return payload.to_vec();
    }
    let user_id = cached_user_id
        .filter(|value| is_valid_user_id(value))
        .map(str::to_owned)
        .unwrap_or_else(generate_fake_user_id);
    let metadata = object
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    metadata
        .as_object_mut()
        .expect("metadata normalized to object")
        .insert("user_id".to_owned(), Value::String(user_id));
    encode_or_original(&root, payload)
}

pub fn inject_claude_system_instructions(payload: &[u8], policy: &ClaudeCloakPolicy) -> Vec<u8> {
    try_inject_claude_system_instructions(payload, policy).unwrap_or_else(|_| payload.to_vec())
}

pub fn try_inject_claude_system_instructions(
    payload: &[u8],
    policy: &ClaudeCloakPolicy,
) -> Result<Vec<u8>, ClaudeCallerSystemBlockError> {
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return Ok(payload.to_vec());
    };
    let original_system = root.get("system").cloned();
    if !policy.strict_mode {
        validate_claude_caller_system_blocks(original_system.as_ref())?;
    }
    let original_parts = collect_forwarded_claude_system_prompt_blocks(original_system.as_ref());
    let fingerprint_text = claude_billing_fingerprint_message_text(payload);
    let entrypoint = if policy.entrypoint.trim().is_empty() {
        parse_claude_entrypoint(&policy.client_user_agent)
    } else {
        policy.entrypoint.trim().to_owned()
    };
    let billing = generate_claude_billing_header(
        payload,
        policy.sign_cch,
        &policy.billing_version,
        &fingerprint_text,
        &entrypoint,
        &policy.workload,
    );
    let Some(object) = root.as_object_mut() else {
        return Ok(payload.to_vec());
    };
    object.insert(
        "system".to_owned(),
        Value::Array(vec![
            json!({"type":"text","text":billing}),
            json!({"type":"text","text":"You are Claude Code, Anthropic's official CLI for Claude.","cache_control":{"type":"ephemeral"}}),
        ]),
    );
    if !policy.strict_mode && !original_parts.is_empty() {
        if claude_uses_legacy_system_reminder(&root) {
            prepend_claude_system_reminders_to_first_user_message(&mut root, &original_parts);
        } else {
            insert_claude_mid_conversation_system_messages(&mut root, &original_parts);
        }
    }
    inject_claude_code_current_date(&mut root, policy.current_date.as_deref(), policy.timezone);
    Ok(encode_or_original(&root, payload))
}

pub fn sanitize_forwarded_claude_system_prompt(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    "Use the available tools when needed to help with software engineering tasks.\nKeep responses concise and focused on the user's request.\nPrefer acting on the user's task over describing product-specific workflows."
        .to_owned()
}

fn system_text_parts(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(text)) if !text.trim().is_empty() => vec![text.trim().to_owned()],
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

pub fn validate_claude_caller_system_blocks(
    value: Option<&Value>,
) -> Result<(), ClaudeCallerSystemBlockError> {
    let Some(Value::Array(parts)) = value else {
        return Ok(());
    };
    for (index, part) in parts.iter().enumerate() {
        if part.get("type").and_then(Value::as_str) != Some("text") {
            return Err(ClaudeCallerSystemBlockError {
                index,
                block_type: part
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("unknown")
                    .to_owned(),
            });
        }
    }
    Ok(())
}

/// Keeps a cloaked `count_tokens` request in Claude Code's measured shape.
///
/// Unlike the Messages path, token counting must not install the synthetic
/// Claude Code top-level system blocks. Caller system text still contributes
/// to the measurement, so non-strict policy relocates each block into the
/// message sequence using the same legacy/mid-system split as generation.
pub fn relocate_claude_system_prompt_for_count_tokens(
    payload: &[u8],
    strict_mode: bool,
) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    let Some(object) = root.as_object_mut() else {
        return payload.to_vec();
    };
    let Some(system) = object.remove("system") else {
        return payload.to_vec();
    };
    let forwarded = if strict_mode {
        Vec::new()
    } else {
        collect_forwarded_claude_system_prompt_blocks(Some(&system))
    };
    if !forwarded.is_empty() {
        if claude_uses_legacy_system_reminder(&root) {
            prepend_claude_system_reminders_to_first_user_message(&mut root, &forwarded);
        } else {
            insert_claude_mid_conversation_system_messages(&mut root, &forwarded);
        }
    }
    encode_or_original(&root, payload)
}

fn collect_forwarded_claude_system_prompt_blocks(value: Option<&Value>) -> Vec<String> {
    system_text_parts(value)
        .into_iter()
        .filter(|text| {
            !text.starts_with("x-anthropic-billing-header:")
                && text != "You are Claude Code, Anthropic's official CLI for Claude."
        })
        .collect()
}

const CLAUDE_LEGACY_SYSTEM_REMINDER_MODELS: &[&str] = &[
    "claude-3-5-haiku-20241022",
    "claude-3-5-haiku-latest",
    "claude-3-7-sonnet-20250219",
    "claude-3-7-sonnet-latest",
    "claude-haiku-4-5",
    "claude-haiku-4-5-20251001",
    "claude-opus-4",
    "claude-opus-4-20250514",
    "claude-opus-4-1",
    "claude-opus-4-1-20250805",
    "claude-opus-4-5",
    "claude-opus-4-5-20251101",
    "claude-opus-4-6",
    "claude-opus-4-7",
    "claude-sonnet-4",
    "claude-sonnet-4-20250514",
    "claude-sonnet-4-5",
    "claude-sonnet-4-5-20250929",
    "claude-sonnet-4-6",
];

fn claude_uses_legacy_system_reminder(root: &Value) -> bool {
    let model = root
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    CLAUDE_LEGACY_SYSTEM_REMINDER_MODELS.contains(&model.as_str())
}

fn claude_caller_system_reminder(text: &str) -> String {
    format!(
        "<system-reminder>\n{text}{}\n</system-reminder>",
        if text.ends_with('\n') { "" } else { "\n" }
    )
}

fn first_claude_user_message_index(root: &Value) -> Option<usize> {
    root.get("messages")?
        .as_array()?
        .iter()
        .position(|message| message.get("role").and_then(Value::as_str) == Some("user"))
}

fn prepend_claude_system_reminders_to_first_user_message(root: &mut Value, texts: &[String]) {
    let Some(index) = first_claude_user_message_index(root) else {
        return;
    };
    let Some(content) = root
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .and_then(|messages| messages.get_mut(index))
        .and_then(|message| message.get_mut("content"))
    else {
        return;
    };
    let reminders = texts
        .iter()
        .map(|text| json!({"type":"text","text":claude_caller_system_reminder(text)}))
        .collect::<Vec<_>>();
    match content {
        Value::String(text) => {
            let mut blocks = reminders;
            blocks.push(json!({"type":"text","text":text}));
            *content = Value::Array(blocks);
        }
        Value::Array(blocks) => {
            let existing = blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>();
            let mut missing = reminders
                .into_iter()
                .filter(|block| {
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .is_none_or(|text| !existing.contains(&text))
                })
                .collect::<Vec<_>>();
            let insert_at = blocks
                .iter()
                .take_while(|block| {
                    block.get("type").and_then(Value::as_str) == Some("tool_result")
                })
                .count();
            blocks.splice(insert_at..insert_at, missing.drain(..));
        }
        _ => {}
    }
}

fn insert_claude_mid_conversation_system_messages(root: &mut Value, texts: &[String]) {
    let Some(first_user) = first_claude_user_message_index(root) else {
        return;
    };
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    let mut insert_at = first_user + 1;
    while messages
        .get(insert_at)
        .is_some_and(|message| message.get("role").and_then(Value::as_str) == Some("user"))
    {
        insert_at += 1;
    }
    let already_present = texts.iter().enumerate().all(|(offset, text)| {
        messages.get(insert_at + offset).is_some_and(|message| {
            message.get("role").and_then(Value::as_str) == Some("system")
                && message
                    .get("content")
                    .and_then(message_content_text)
                    .as_deref()
                    == Some(text)
        })
    });
    if already_present {
        return;
    }
    let inserted = texts.iter().map(|text| {
        json!({"role":"system","content":[{"type":"text","text":text,"cache_control":{"type":"ephemeral"}}]})
    });
    messages.splice(insert_at..insert_at, inserted);
}

fn message_content_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => Some(
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n\n"),
        ),
        _ => None,
    }
}

fn current_date_in_timezone(timezone: Tz) -> String {
    let now: chrono::DateTime<Utc> = std::time::SystemTime::now().into();
    date_in_timezone_at(now, timezone)
}

fn date_in_timezone_at(now: chrono::DateTime<Utc>, timezone: Tz) -> String {
    let now = now.with_timezone(&timezone);
    format!("{:04}-{:02}-{:02}", now.year(), now.month(), now.day())
}

fn inject_claude_code_current_date(root: &mut Value, current_date: Option<&str>, timezone: Tz) {
    let Some(index) = first_claude_user_message_index(root) else {
        return;
    };
    let date = current_date
        .map(str::trim)
        .filter(|date| !date.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| current_date_in_timezone(timezone));
    let reminder = format!(
        "<system-reminder>\nAs you answer the user's questions, you can use the following context:\n# currentDate\nToday's date is {date}.\n\n      IMPORTANT: this context may or may not be relevant to your tasks. You should not respond to this context unless it is highly relevant to your task.\n</system-reminder>\n\n"
    );
    let Some(content) = root
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .and_then(|messages| messages.get_mut(index))
        .and_then(|message| message.get_mut("content"))
    else {
        return;
    };
    match content {
        Value::String(text) => {
            *content = json!([
                {"type":"text","text":reminder},
                {"type":"text","text":text,"cache_control":{"type":"ephemeral"}}
            ]);
        }
        Value::Array(blocks) => {
            blocks.retain(|block| {
                !block
                    .get("text")
                    .and_then(Value::as_str)
                .is_some_and(|text| {
                    text.starts_with("<system-reminder>\nAs you answer the user's questions, you can use the following context:\n# currentDate\nToday's date is ")
                })
            });
            if let Some(block) = blocks.iter_mut().find(|block| {
                block.get("type").and_then(Value::as_str) == Some("text")
                    && !block
                        .get("text")
                        .and_then(Value::as_str)
                        .is_some_and(|text| text.starts_with("<system-reminder>"))
            }) {
                if let Some(object) = block.as_object_mut() {
                    object.insert("cache_control".to_owned(), json!({"type":"ephemeral"}));
                }
            }
            blocks.insert(0, json!({"type":"text","text":reminder}));
        }
        _ => {}
    }
}

const CLAUDE_CODE_CONTEXT_MANAGEMENT: &str = "clear_thinking_20251015";

pub fn inject_claude_code_context_management(payload: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    let Some(object) = root.as_object_mut() else {
        return payload.to_vec();
    };
    if object.contains_key("context_management") {
        return payload.to_vec();
    }
    object.insert(
        "context_management".to_owned(),
        json!({"edits":[{"type":CLAUDE_CODE_CONTEXT_MANAGEMENT,"keep":"all"}]}),
    );
    encode_or_original(&root, payload)
}

pub fn ensure_claude_cache_control(payload: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    inject_tools_cache_control(&mut root);
    inject_system_cache_control(&mut root);
    inject_messages_cache_control(&mut root);
    encode_or_original(&root, payload)
}

pub fn count_claude_cache_controls(payload: &[u8]) -> usize {
    serde_json::from_slice::<Value>(payload)
        .ok()
        .as_ref()
        .map(count_cache_controls_value)
        .unwrap_or_default()
}

fn count_cache_controls_value(root: &Value) -> usize {
    section_objects(root.get("tools"))
        .chain(section_objects(root.get("system")))
        .chain(message_content_objects(root))
        .filter(|item| item.get("cache_control").is_some())
        .count()
}

fn section_objects(value: Option<&Value>) -> impl Iterator<Item = &Value> {
    value.and_then(Value::as_array).into_iter().flatten()
}

fn message_content_objects(root: &Value) -> impl Iterator<Item = &Value> {
    root.get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|message| message.get("content").and_then(Value::as_array))
        .flatten()
}

fn inject_tools_cache_control(root: &mut Value) {
    let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };
    if tools.iter().any(|tool| tool.get("cache_control").is_some()) {
        return;
    }
    if let Some(tool) = tools.iter_mut().rev().find(|tool| {
        !tool
            .get("defer_loading")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }) {
        if let Some(object) = tool.as_object_mut() {
            object.insert("cache_control".to_owned(), json!({"type":"ephemeral"}));
        }
    }
}

fn inject_system_cache_control(root: &mut Value) {
    let Some(system) = root.get_mut("system") else {
        return;
    };
    match system {
        Value::Array(blocks) => {
            if blocks.is_empty()
                || blocks
                    .iter()
                    .any(|block| block.get("cache_control").is_some())
            {
                return;
            }
            if let Some(object) = blocks.last_mut().and_then(Value::as_object_mut) {
                object.insert("cache_control".to_owned(), json!({"type":"ephemeral"}));
            }
        }
        Value::String(text) => {
            *system = json!([{"type":"text","text":text,"cache_control":{"type":"ephemeral"}}]);
        }
        _ => {}
    }
}

fn inject_messages_cache_control(root: &mut Value) {
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    if messages.iter().any(|message| {
        message
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|blocks| {
                blocks
                    .iter()
                    .any(|block| block.get("cache_control").is_some())
            })
    }) {
        return;
    }
    let users: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.get("role").and_then(Value::as_str) == Some("user"))
        .map(|(index, _)| index)
        .collect();
    if users.len() < 2 {
        return;
    }
    let Some(content) = messages[users[users.len() - 2]].get_mut("content") else {
        return;
    };
    match content {
        Value::Array(blocks) => {
            if let Some(object) = blocks.last_mut().and_then(Value::as_object_mut) {
                object.insert("cache_control".to_owned(), json!({"type":"ephemeral"}));
            }
        }
        Value::String(text) => {
            *content = json!([{"type":"text","text":text,"cache_control":{"type":"ephemeral"}}]);
        }
        _ => {}
    }
}

pub fn normalize_claude_cache_control_ttl(payload: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    let mut seen_five_minutes = false;
    let mut changed = false;
    for section in ["tools", "system"] {
        if let Some(items) = root.get_mut(section).and_then(Value::as_array_mut) {
            for item in items {
                normalize_cache_item(item, &mut seen_five_minutes, &mut changed);
            }
        }
    }
    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            if let Some(items) = message.get_mut("content").and_then(Value::as_array_mut) {
                for item in items {
                    normalize_cache_item(item, &mut seen_five_minutes, &mut changed);
                }
            }
        }
    }
    if changed {
        encode_or_original(&root, payload)
    } else {
        payload.to_vec()
    }
}

fn normalize_cache_item(item: &mut Value, seen_five_minutes: &mut bool, changed: &mut bool) {
    let Some(cache) = item.get_mut("cache_control") else {
        return;
    };
    let Some(cache) = cache.as_object_mut() else {
        *seen_five_minutes = true;
        return;
    };
    let is_one_hour = cache.get("ttl").and_then(Value::as_str) == Some("1h");
    if !is_one_hour {
        *seen_five_minutes = true;
    } else if *seen_five_minutes {
        cache.remove("ttl");
        *changed = true;
    }
}

pub fn enforce_claude_cache_control_limit(payload: &[u8], maximum: usize) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    let total = count_cache_controls_value(&root);
    if total <= maximum {
        return payload.to_vec();
    }
    let mut excess = total - maximum;
    strip_non_last_section(&mut root, "system", &mut excess);
    strip_non_last_section(&mut root, "tools", &mut excess);
    strip_message_controls(&mut root, &mut excess);
    strip_all_section(&mut root, "system", &mut excess);
    strip_all_section(&mut root, "tools", &mut excess);
    encode_or_original(&root, payload)
}

fn strip_non_last_section(root: &mut Value, section: &str, excess: &mut usize) {
    if *excess == 0 {
        return;
    }
    let Some(items) = root.get_mut(section).and_then(Value::as_array_mut) else {
        return;
    };
    let last = items
        .iter()
        .rposition(|item| item.get("cache_control").is_some());
    for (index, item) in items.iter_mut().enumerate() {
        if *excess == 0 {
            break;
        }
        if Some(index) != last && item.get("cache_control").is_some() {
            if let Some(object) = item.as_object_mut() {
                object.remove("cache_control");
                *excess -= 1;
            }
        }
    }
}

fn strip_message_controls(root: &mut Value, excess: &mut usize) {
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        let Some(items) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        for item in items {
            if *excess == 0 {
                return;
            }
            if item.get("cache_control").is_some() {
                if let Some(object) = item.as_object_mut() {
                    object.remove("cache_control");
                    *excess -= 1;
                }
            }
        }
    }
}

fn strip_all_section(root: &mut Value, section: &str, excess: &mut usize) {
    let Some(items) = root.get_mut(section).and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        if *excess == 0 {
            return;
        }
        if item.get("cache_control").is_some() {
            if let Some(object) = item.as_object_mut() {
                object.remove("cache_control");
                *excess -= 1;
            }
        }
    }
}

fn encode_or_original(value: &Value, original: &[u8]) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_else(|_| original.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoint_and_fingerprint_match_pinned_semantics() {
        assert_eq!(
            parse_claude_entrypoint("claude-cli/2 (external, vscode)"),
            "vscode"
        );
        assert_eq!(parse_claude_entrypoint("other"), "cli");
        assert_eq!(compute_claude_fingerprint("short", "2.1.63").len(), 3);
    }

    #[test]
    fn legacy_system_blocks_are_preserved_as_separate_reminders() {
        let mut policy = ClaudeCloakPolicy::oauth_default();
        policy.current_date = Some("2026-08-04".to_owned());
        let output = inject_claude_system_instructions(
            br#"{"model":"claude-opus-4-6","system":[{"type":"text","text":"first"},{"type":"text","text":"second"}],"messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"x"}]}]}"#,
            &policy,
        );
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["system"].as_array().unwrap().len(), 2);
        assert!(value["messages"][0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("2026-08-04"));
        assert_eq!(value["messages"][0]["content"][1]["type"], "tool_result");
        assert!(value["messages"][0]["content"][2]["text"]
            .as_str()
            .unwrap()
            .contains("first"));
        assert!(value["messages"][0]["content"][3]["text"]
            .as_str()
            .unwrap()
            .contains("second"));
        assert!(value["system"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("x-anthropic-billing-header:"));
    }

    #[test]
    fn current_models_receive_authoritative_mid_conversation_system_messages() {
        let mut policy = ClaudeCloakPolicy::oauth_default();
        policy.current_date = Some("2026-08-04".to_owned());
        let output = inject_claude_system_instructions(
            br#"{"model":"claude-opus-5","system":[{"type":"text","text":"first"},{"type":"text","text":"second"}],"messages":[{"role":"user","content":"hello"},{"role":"assistant","content":"answer"}]}"#,
            &policy,
        );
        let value: Value = serde_json::from_slice(&output).unwrap();
        let messages = value["messages"].as_array().unwrap();
        assert_eq!(
            messages
                .iter()
                .map(|message| message["role"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["user", "system", "system", "assistant"]
        );
        assert_eq!(messages[1]["content"][0]["text"], "first");
        assert_eq!(messages[2]["content"][0]["text"], "second");
    }

    #[test]
    fn non_text_caller_system_block_is_typed_request_scoped_400() {
        let error = try_apply_claude_cloaking(
            br#"{"model":"claude-opus-5","system":[{"type":"input_image"}],"messages":[{"role":"user","content":"hello"}]}"#,
            "claude-opus-5",
            &ClaudeCloakPolicy::oauth_default(),
            None,
        )
        .unwrap_err();
        assert_eq!(error.index(), 0);
        assert_eq!(error.block_type(), "input_image");
        assert_eq!(StatusError::status_code(&error), 400);
        assert!(RequestScopedError::is_request_scoped(&error));
    }

    #[test]
    fn strict_mode_can_drop_non_text_system_and_verified_client_is_byte_passthrough() {
        let input = br#"{"model":"claude-opus-5","system":[{"type":"input_image"}],"messages":[{"role":"user","content":"hello"}]}"#;
        let mut strict = ClaudeCloakPolicy::oauth_default();
        strict.strict_mode = true;
        assert!(try_apply_claude_cloaking(input, "claude-opus-5", &strict, None).is_ok());

        let mut verified = ClaudeCloakPolicy::oauth_default();
        verified.mode = "always".to_owned();
        verified.verified_claude_code = true;
        assert_eq!(
            try_apply_claude_cloaking(input, "claude-opus-5", &verified, None).unwrap(),
            input
        );
    }

    #[test]
    fn candidate_auto_policy_cloaks_unconfirmed_user_agent_and_never_bypasses() {
        let input = br#"{"model":"claude-opus-5","messages":[{"role":"user","content":"hello"}]}"#;
        let mut copied_user_agent = ClaudeCloakPolicy::oauth_default();
        copied_user_agent.client_user_agent = "claude-cli/2.1.220 (external, cli)".to_owned();
        let cloaked =
            try_apply_claude_cloaking(input, "claude-opus-5", &copied_user_agent, None).unwrap();
        assert_ne!(cloaked, input);

        copied_user_agent.mode = "never".to_owned();
        assert_eq!(
            try_apply_claude_cloaking(input, "claude-opus-5", &copied_user_agent, None,).unwrap(),
            input
        );
    }

    #[test]
    fn candidate_context_management_and_fallback_billing_are_exact() {
        let caller_owned = br#"{"context_management":{"edits":[]}}"#;
        assert_eq!(
            inject_claude_code_context_management(caller_owned),
            caller_owned
        );
        let injected = inject_claude_code_context_management(br#"{"model":"claude-opus-5"}"#);
        let value: Value = serde_json::from_slice(&injected).unwrap();
        assert_eq!(
            value["context_management"],
            json!({"edits":[{"type":"clear_thinking_20251015","keep":"all"}]})
        );
        let header = claude_cch_fallback_billing_header(
            br#"{"messages":[{"role":"user","content":"last user"}]}"#,
            "2.1.220",
            "sdk-cli",
            "",
        );
        assert!(header.contains("cc_version=2.1.220."));
        assert!(header.contains("cc_entrypoint=sdk-cli; cch=00000;"));
    }

    #[test]
    fn candidate_current_date_reminder_text_is_exact() {
        let mut policy = ClaudeCloakPolicy::oauth_default();
        policy.current_date = Some("2026-08-01".to_owned());
        let output = inject_claude_system_instructions(
            br#"{"model":"claude-opus-5","messages":[{"role":"user","content":"hello"}]}"#,
            &policy,
        );
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            value["messages"][0]["content"][0]["text"],
            "<system-reminder>\nAs you answer the user's questions, you can use the following context:\n# currentDate\nToday's date is 2026-08-01.\n\n      IMPORTANT: this context may or may not be relevant to your tasks. You should not respond to this context unless it is highly relevant to your task.\n</system-reminder>\n\n"
        );
        assert!(value["messages"][0]["content"][0]
            .get("cache_control")
            .is_none());
    }

    #[test]
    fn candidate_current_date_uses_typed_credential_timezone() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-04T01:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            date_in_timezone_at(now, chrono_tz::Pacific::Honolulu),
            "2026-08-03"
        );
        assert_eq!(
            date_in_timezone_at(now, chrono_tz::Europe::Berlin),
            "2026-08-04"
        );
    }

    #[test]
    fn cache_injection_uses_last_eligible_tool_system_and_second_last_user() {
        let input = br#"{"tools":[{"name":"a"},{"name":"b","defer_loading":true}],"system":[{"type":"text","text":"s"}],"messages":[{"role":"user","content":"one"},{"role":"assistant","content":"ok"},{"role":"user","content":"two"}]}"#;
        let output = ensure_claude_cache_control(input);
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert!(value["tools"][0].get("cache_control").is_some());
        assert!(value["tools"][1].get("cache_control").is_none());
        assert!(value["system"][0].get("cache_control").is_some());
        assert!(value["messages"][0]["content"][0]
            .get("cache_control")
            .is_some());
    }

    #[test]
    fn ttl_normalization_is_ordered_and_noop_is_byte_identical() {
        let no_change = br#"{"tools":[{"cache_control":{"type":"ephemeral","ttl":"1h"}}],"system":[{"text":"<x>&","cache_control":{"type":"ephemeral","ttl":"1h"}}]}"#;
        assert_eq!(normalize_claude_cache_control_ttl(no_change), no_change);
        let changed = normalize_claude_cache_control_ttl(br#"{"tools":[{"cache_control":{"type":"ephemeral"}}],"system":[{"cache_control":{"type":"ephemeral","ttl":"1h"}}]}"#);
        let value: Value = serde_json::from_slice(&changed).unwrap();
        assert!(value["system"][0]["cache_control"].get("ttl").is_none());
    }

    #[test]
    fn cache_limit_preserves_last_tool_before_messages() {
        let output = enforce_claude_cache_control_limit(
            br#"{"tools":[{"name":"a","cache_control":{}},{"name":"b","cache_control":{}}],"system":[{"cache_control":{}}],"messages":[{"content":[{"cache_control":{}},{"cache_control":{}}]}]}"#,
            4,
        );
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert!(value["tools"][0].get("cache_control").is_none());
        assert!(value["tools"][1].get("cache_control").is_some());
        assert_eq!(count_claude_cache_controls(&output), 4);
    }
}
