// ref: internal/runtime/executor/claude_executor_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{HashMap, HashSet};

use super::claude_executor_cloaking::{
    is_explicit_claude_prompt_cache_mode, strip_claude_prompt_cache_options,
};
use serde_json::Value;

use super::helps::{
    augment_claude_builtin_tool_registry, claude_mcp_tool_alias, is_claude_mcp_tool_name,
    is_claude_server_tool_type,
};
use super::{
    claude_cch_fallback_billing_header, count_claude_cache_controls,
    enforce_claude_cache_control_limit, ensure_claude_cache_control,
    finalize_anthropic_messages_body_cch, normalize_claude_cache_control_ttl,
    sign_anthropic_messages_body,
};
use crate::internal::signature::sanitize_claude_messages_for_claude_upstream;

#[cfg(feature = "anthropic-fingerprint-transport")]
use std::fmt;
#[cfg(feature = "anthropic-fingerprint-transport")]
use std::future::Future;
#[cfg(feature = "anthropic-fingerprint-transport")]
use std::pin::Pin;
#[cfg(feature = "anthropic-fingerprint-transport")]
use std::time::{Duration, SystemTime};

#[cfg(feature = "anthropic-fingerprint-transport")]
use futures_util::StreamExt;
#[cfg(feature = "anthropic-fingerprint-transport")]
use tokio::sync::mpsc;
#[cfg(feature = "anthropic-fingerprint-transport")]
use wreq::header::{ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, USER_AGENT};
#[cfg(feature = "anthropic-fingerprint-transport")]
use wreq::{Client, RequestBuilder};

#[cfg(feature = "anthropic-fingerprint-transport")]
use super::claude_executor::{
    decode_claude_response_body, ClaudeAuthorizationHeader, ClaudeCredentialMode,
    ClaudeMessagesRequest, ClaudeMessagesResponse, ClaudeMessagesStreamResponse,
    ClaudeMessagesStreamingTransport, ClaudeMessagesTransport, ClaudeMessagesTransportFailure,
};
#[cfg(feature = "anthropic-fingerprint-transport")]
use crate::internal::auth::claude::utls_transport::{
    build_anthropic_messages_client, AnthropicTransportBuildError,
};
#[cfg(feature = "anthropic-fingerprint-transport")]
use crate::internal::runtime::executor::helps::{
    CLAUDE_CODE_COUNT_TOKENS_HEADER_ORDER, CLAUDE_CODE_MESSAGES_HEADER_ORDER,
};
#[cfg(feature = "anthropic-fingerprint-transport")]
use crate::sdk::cliproxy::executor::Headers;

#[cfg(feature = "anthropic-fingerprint-transport")]
const ANTHROPIC_VERSION: &str = "2023-06-01";
#[cfg(feature = "anthropic-fingerprint-transport")]
const CLAUDE_CODE_TIMEOUT_HEADER: &str = "600";
const CLAUDE_FAST_MODE_BETA: &str = "fast-mode-2026-02-01";
const CLAUDE_CODE_BETA: &str = "claude-code-20250219";
const CLAUDE_OAUTH_BETA: &str = "oauth-2025-04-20";
const CLAUDE_EXTENDED_CACHE_TTL_BETA: &str = "extended-cache-ttl-2025-04-11";
const CLAUDE_REDACT_THINKING_BETA: &str = "redact-thinking-2026-02-12";
const CLAUDE_TOKEN_COUNTING_BETA: &str = "token-counting-2024-11-01";

const OAUTH_TOOL_RENAMES: [(&str, &str); 14] = [
    ("bash", "Bash"),
    ("read", "Read"),
    ("write", "Write"),
    ("edit", "Edit"),
    ("glob", "Glob"),
    ("grep", "Grep"),
    ("task", "Task"),
    ("webfetch", "WebFetch"),
    ("todowrite", "TodoWrite"),
    ("question", "Question"),
    ("skill", "Skill"),
    ("ls", "LS"),
    ("todoread", "TodoRead"),
    ("notebookedit", "NotebookEdit"),
];

/// Builds the measured Claude Code 2.1.220 beta profile in wire order. Body
/// capabilities, not a process-global constant, decide conditional betas.
pub fn claude_code_cli_betas(body: &[u8], requested: &HashSet<String>, oauth: bool) -> String {
    let root = serde_json::from_slice::<Value>(body).unwrap_or(Value::Null);
    let mut betas = vec![CLAUDE_CODE_BETA];
    if oauth {
        betas.push(CLAUDE_OAUTH_BETA);
    }
    if requested.contains("context-1m-2025-08-07") {
        betas.push("context-1m-2025-08-07");
    }
    betas.push("interleaved-thinking-2025-05-14");
    if root
        .pointer("/thinking/display")
        .and_then(Value::as_str)
        .is_none_or(|value| value.trim().is_empty())
    {
        betas.push(CLAUDE_REDACT_THINKING_BETA);
    }
    betas.extend([
        "thinking-token-count-2026-05-13",
        "context-management-2025-06-27",
        "prompt-caching-scope-2026-01-05",
    ]);
    if !claude_uses_legacy_system_reminder(&root) {
        betas.push("mid-conversation-system-2026-04-07");
    }
    if root
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty())
    {
        betas.push("advanced-tool-use-2025-11-20");
    }
    betas.push("effort-2025-11-24");
    if oauth && !requested.contains("fallback-credit-2026-06-01") {
        betas.push("fallback-credit-2026-06-01");
    }
    for beta in [
        "server-side-fallback-2026-06-01",
        "fallback-credit-2026-06-01",
        "structured-outputs-2025-12-15",
    ] {
        if requested.contains(beta) {
            betas.push(beta);
        }
    }
    if claude_request_uses_fast_mode(body, requested) {
        betas.push(CLAUDE_FAST_MODE_BETA);
    }
    if oauth {
        betas.push(CLAUDE_EXTENDED_CACHE_TTL_BETA);
    }
    if root.get("diagnostics").and_then(Value::as_object).is_some() {
        betas.push("cache-diagnosis-2026-04-07");
    }
    betas.join(",")
}

pub fn claude_count_tokens_betas(oauth: bool) -> String {
    let mut betas = vec![CLAUDE_CODE_BETA];
    if oauth {
        betas.push(CLAUDE_OAUTH_BETA);
    }
    betas.extend([
        "interleaved-thinking-2025-05-14",
        "context-management-2025-06-27",
        CLAUDE_TOKEN_COUNTING_BETA,
    ]);
    betas.join(",")
}

pub fn claude_requested_betas(header: &str, extra: &[String]) -> HashSet<String> {
    header
        .split(',')
        .chain(extra.iter().map(String::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn claude_request_uses_fast_mode(body: &[u8], requested: &HashSet<String>) -> bool {
    requested.contains(CLAUDE_FAST_MODE_BETA)
        || serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|root| root.get("speed").and_then(Value::as_str).map(str::to_owned))
            .is_some_and(|speed| speed.trim().eq_ignore_ascii_case("fast"))
}

pub fn with_claude_oauth_credential_betas(header: &str) -> String {
    let mut parts: Vec<String> = header
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect();
    parts.dedup();
    if !parts.iter().any(|beta| beta == CLAUDE_OAUTH_BETA) {
        let index = usize::from(parts.first().is_some_and(|beta| beta == CLAUDE_CODE_BETA));
        parts.insert(index, CLAUDE_OAUTH_BETA.to_owned());
    }
    if !parts
        .iter()
        .any(|beta| beta == CLAUDE_EXTENDED_CACHE_TTL_BETA)
    {
        parts.push(CLAUDE_EXTENDED_CACHE_TTL_BETA.to_owned());
    }
    parts.join(",")
}

fn claude_uses_legacy_system_reminder(root: &Value) -> bool {
    root.get("messages")
        .and_then(Value::as_array)
        .is_some_and(|messages| {
            messages.iter().skip(1).any(|message| {
                message.get("role").and_then(Value::as_str) == Some("user")
                    && message
                        .get("content")
                        .is_some_and(|content| content.to_string().contains("system-reminder"))
            })
        })
}

pub fn claude_wire_header_name(name: &str) -> &str {
    match name.to_ascii_lowercase().as_str() {
        "content-type" => "content-type",
        "authorization" => "authorization",
        "anthropic-version" => "anthropic-version",
        "anthropic-beta" => "anthropic-beta",
        "x-api-key" => "x-api-key",
        "x-app" => "x-app",
        "x-stainless-retry-count" => "x-stainless-retry-count",
        "x-stainless-timeout" => "x-stainless-timeout",
        "x-stainless-lang" => "x-stainless-lang",
        "x-stainless-package-version" => "x-stainless-package-version",
        "x-stainless-os" => "x-stainless-os",
        "x-stainless-arch" => "x-stainless-arch",
        "x-stainless-runtime" => "x-stainless-runtime",
        "x-stainless-runtime-version" => "x-stainless-runtime-version",
        "x-client-request-id" => "x-client-request-id",
        "x-claude-code-session-id" => "x-claude-code-session-id",
        _ => name,
    }
}

pub fn extract_and_remove_claude_betas(body: &[u8]) -> (Vec<String>, Vec<u8>) {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return (Vec::new(), body.to_vec());
    };
    let Some(object) = root.as_object_mut() else {
        return (Vec::new(), body.to_vec());
    };
    let Some(value) = object.remove("betas") else {
        return (Vec::new(), body.to_vec());
    };
    let betas = match value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| json_text(&item))
            .filter_map(non_empty_trimmed)
            .collect(),
        value => json_text(&value)
            .and_then(non_empty_trimmed)
            .into_iter()
            .collect(),
    };
    (betas, encode_or_original(&root, body))
}

pub fn prepare_claude_upstream_body(
    body: &[u8],
    model: Option<&crate::internal::registry::ModelInfo>,
) -> (Vec<u8>, Vec<String>, HashMap<String, String>) {
    prepare_claude_upstream_body_with_identity(body, model, "", true)
}

pub fn prepare_claude_upstream_body_with_identity(
    body: &[u8],
    model: Option<&crate::internal::registry::ModelInfo>,
    caller_secret: &str,
    oauth: bool,
) -> (Vec<u8>, Vec<String>, HashMap<String, String>) {
    // ref: internal/runtime/executor/claude_executor_execute.go:94-199 @ eb6a768d103da08c039c20dfad28aad2c936fbbc
    let explicit_cache = is_explicit_claude_prompt_cache_mode(&[body]);
    let body = sanitize_claude_web_search_domains(body);
    let body = disable_claude_thinking_if_tool_choice_forced(&body);
    let body = normalize_claude_sampling_for_upstream(&body);
    let body = ensure_claude_model_max_tokens(&body, model);
    let body = if model.is_some_and(|model| model.provider_type.eq_ignore_ascii_case("claude")) {
        sanitize_claude_messages_for_claude_upstream(&body, false).0
    } else {
        body
    };
    let body = if !explicit_cache && count_claude_cache_controls(&body) == 0 {
        ensure_claude_cache_control(&body)
    } else {
        body
    };
    let body = enforce_claude_cache_control_limit(&body, 4);
    let body = if explicit_cache {
        body
    } else {
        normalize_claude_cache_control_ttl(&body)
    };
    let body = strip_claude_prompt_cache_options(&body);
    let (requested_betas, body) = extract_and_remove_claude_betas(&body);
    let requested = claude_requested_betas("", &requested_betas);
    let betas = claude_code_cli_betas(&body, &requested, oauth)
        .split(',')
        .map(str::to_owned)
        .collect();
    let (body, reverse) = if oauth && !caller_secret.is_empty() {
        // The per-caller secret is used only as HMAC-like aliasing material and
        // never leaves this function. Unlike the legacy static-name pass this
        // covers arbitrary MCP declarations and every matching history/tool
        // choice reference in one deterministic mapping.
        remap_claude_oauth_tool_names_with_secret(&body, caller_secret)
    } else {
        prepare_claude_oauth_tool_names_for_upstream(&body, "", false)
    };
    // Candidate signs only after every payload/config/tool-name rewrite. If a
    // verified client did not carry the measured billing block, install the
    // deterministic Claude Code fallback before hashing the final body.
    let body = if oauth {
        let fallback = claude_cch_fallback_billing_header(&body, "2.1.280", "cli", "");
        finalize_anthropic_messages_body_cch(&body, &fallback)
    } else {
        sign_anthropic_messages_body(&body)
    };
    (body, betas, reverse)
}

/// Removes generation-only fields rejected by Anthropic's first-party
/// `count_tokens` endpoint while preserving the caller's messages and tools.
pub fn prepare_claude_first_party_count_tokens_body(body: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(object) = root.as_object_mut() else {
        return body.to_vec();
    };
    object.remove("metadata");
    object.remove("context_management");
    object.remove("diagnostics");
    object.remove("prompt_cache_options");
    encode_or_original(&root, body)
}

pub fn append_claude_fast_mode_beta(body: &[u8], mut betas: Vec<String>) -> Vec<String> {
    let fast = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|root| root.get("speed").and_then(Value::as_str).map(str::to_owned))
        .is_some_and(|speed| speed.trim().eq_ignore_ascii_case("fast"));
    if fast
        && !betas
            .iter()
            .any(|beta| beta.trim() == CLAUDE_FAST_MODE_BETA)
    {
        betas.push(CLAUDE_FAST_MODE_BETA.to_owned());
    }
    betas
}

pub fn disable_claude_thinking_if_tool_choice_forced(body: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let forced = root
        .pointer("/tool_choice/type")
        .and_then(Value::as_str)
        .is_some_and(|kind| matches!(kind, "any" | "tool"));
    if !forced {
        return body.to_vec();
    }
    let Some(object) = root.as_object_mut() else {
        return body.to_vec();
    };
    object.remove("thinking");
    if let Some(output) = object
        .get_mut("output_config")
        .and_then(Value::as_object_mut)
    {
        output.remove("effort");
        if output.is_empty() {
            object.remove("output_config");
        }
    }
    encode_or_original(&root, body)
}

pub fn normalize_claude_sampling_for_upstream(body: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let thinking = root
        .pointer("/thinking/type")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase());
    let Some(object) = root.as_object_mut() else {
        return body.to_vec();
    };
    let mut changed = object.remove("temperature").is_some();
    changed |= object.remove("top_p").is_some();
    if thinking
        .as_deref()
        .is_some_and(|kind| matches!(kind, "enabled" | "adaptive" | "auto"))
    {
        changed |= object.remove("top_k").is_some();
    }
    if changed {
        encode_or_original(&root, body)
    } else {
        body.to_vec()
    }
}

pub fn sanitize_claude_web_search_domains(body: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let mut changed = false;
    if let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            let Some(object) = tool.as_object_mut() else {
                continue;
            };
            if !object
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.starts_with("web_search_"))
            {
                continue;
            }
            for key in ["allowed_domains", "blocked_domains"] {
                if object
                    .get(key)
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
                {
                    object.remove(key);
                    changed = true;
                }
            }
        }
    }
    if changed {
        encode_or_original(&root, body)
    } else {
        body.to_vec()
    }
}

pub fn rebuild_mid_claude_system_messages(body: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return body.to_vec();
    };
    let mut moved = Vec::new();
    let mut kept = Vec::with_capacity(messages.len());
    for message in messages.drain(..) {
        if message
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| role.trim().eq_ignore_ascii_case("system"))
        {
            moved.extend(claude_system_text_parts(message.get("content")));
        } else {
            kept.push(message);
        }
    }
    if moved.is_empty() {
        return body.to_vec();
    }
    *messages = kept;
    let mut system = claude_system_text_parts(root.get("system"));
    system.extend(moved);
    if let Some(object) = root.as_object_mut() {
        object.insert("system".to_owned(), Value::Array(system));
    }
    encode_or_original(&root, body)
}

pub fn ensure_claude_model_max_tokens(
    body: &[u8],
    model: Option<&crate::internal::registry::ModelInfo>,
) -> Vec<u8> {
    let Some(model) = model else {
        return body.to_vec();
    };
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(object) = root.as_object_mut() else {
        return body.to_vec();
    };
    if object.contains_key("max_tokens") {
        return body.to_vec();
    }
    let maximum = if model.max_completion_tokens > 0 {
        model.max_completion_tokens
    } else {
        1024
    };
    object.insert("max_tokens".to_owned(), Value::from(maximum));
    encode_or_original(&root, body)
}

fn claude_system_text_parts(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            vec![serde_json::json!({"type":"text","text":text})]
        }
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| match part {
                Value::String(text) if !text.trim().is_empty() => {
                    Some(serde_json::json!({"type":"text","text":text}))
                }
                Value::Object(_)
                    if part.get("type").and_then(Value::as_str) == Some("text")
                        && part
                            .get("text")
                            .and_then(Value::as_str)
                            .is_some_and(|text| !text.trim().is_empty()) =>
                {
                    Some(part.clone())
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub fn remap_claude_oauth_tool_names(body: &[u8]) -> (Vec<u8>, HashMap<String, String>) {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return (body.to_vec(), HashMap::new());
    };
    let mapping: HashMap<&str, &str> = OAUTH_TOOL_RENAMES.into_iter().collect();
    let mut reverse = HashMap::new();
    let mut changed = false;
    if let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            if tool
                .get("type")
                .is_some_and(|value| !json_text(value).unwrap_or_default().is_empty())
            {
                continue;
            }
            changed |= rename_object_field(tool, "name", &mapping, &mut reverse);
        }
    }
    if root.pointer("/tool_choice/type").and_then(Value::as_str) == Some("tool") {
        if let Some(choice) = root.get_mut("tool_choice") {
            changed |= rename_object_field(choice, "name", &mapping, &mut reverse);
        }
    }
    visit_message_tool_names(&mut root, |object, key| {
        let did_change = rename_object_field(object, key, &mapping, &mut reverse);
        changed |= did_change;
    });
    let output = if changed {
        encode_or_original(&root, body)
    } else {
        body.to_vec()
    };
    (output, reverse)
}

/// Maps arbitrary caller tool declarations onto deterministic Claude Code MCP
/// names and rewrites only references to declarations from this request.
pub fn remap_claude_oauth_tool_names_with_secret(
    body: &[u8],
    caller_secret: &str,
) -> (Vec<u8>, HashMap<String, String>) {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return (body.to_vec(), HashMap::new());
    };
    let mut forward = HashMap::<String, String>::new();
    let mut reverse = HashMap::<String, String>::new();
    let mut occupied = HashSet::<String>::new();
    let mut protected = HashSet::<String>::new();
    if let Some(tools) = root.get("tools").and_then(Value::as_array) {
        occupied.extend(
            tools
                .iter()
                .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_owned)),
        );
        for tool in tools {
            let server_tool = tool
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(is_claude_server_tool_type);
            let Some(original) = tool.get("name").and_then(Value::as_str) else {
                continue;
            };
            if server_tool {
                protected.insert(original.to_owned());
                continue;
            }
            if is_claude_mcp_tool_name(original) {
                continue;
            }
            let mut attempt = 0;
            let alias = loop {
                let candidate = claude_mcp_tool_alias(caller_secret, original, attempt);
                if !occupied.contains(&candidate) {
                    break candidate;
                }
                attempt += 1;
            };
            occupied.insert(alias.clone());
            forward.insert(original.to_owned(), alias.clone());
            reverse.insert(alias, original.to_owned());
        }
    }
    if forward.is_empty() {
        return (body.to_vec(), reverse);
    }
    if let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            if tool
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(is_claude_server_tool_type)
            {
                continue;
            }
            replace_name_from_map(tool, "name", &forward);
        }
    }
    if root.pointer("/tool_choice/type").and_then(Value::as_str) == Some("tool") {
        if let Some(choice) = root.get_mut("tool_choice") {
            replace_name_from_map(choice, "name", &forward);
        }
    }
    visit_message_tool_names(&mut root, |object, key| {
        let is_protected = object
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|name| protected.contains(name));
        if !is_protected {
            replace_name_from_map(object, key, &forward);
        }
    });
    (encode_or_original(&root, body), reverse)
}

fn replace_name_from_map(value: &mut Value, key: &str, mapping: &HashMap<String, String>) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let Some(current) = object.get(key).and_then(Value::as_str) else {
        return false;
    };
    let Some(replacement) = mapping.get(current) else {
        return false;
    };
    object.insert(key.to_owned(), Value::String(replacement.clone()));
    true
}

pub fn prepare_claude_oauth_tool_names_for_upstream(
    body: &[u8],
    prefix: &str,
    prefix_disabled: bool,
) -> (Vec<u8>, HashMap<String, String>) {
    let (body, reverse) = remap_claude_oauth_tool_names(body);
    let body = if prefix_disabled {
        body
    } else {
        apply_claude_tool_prefix(&body, prefix)
    };
    (body, reverse)
}

pub fn reverse_remap_claude_oauth_tool_names(
    body: &[u8],
    reverse: &HashMap<String, String>,
) -> Vec<u8> {
    mutate_response_tool_names(body, |name| reverse.get(name).cloned())
}

pub fn reverse_remap_claude_oauth_tool_names_from_stream_line(
    line: &[u8],
    reverse: &HashMap<String, String>,
) -> Vec<u8> {
    mutate_stream_tool_name(line, |name| reverse.get(name).cloned())
}

pub fn apply_claude_tool_prefix(body: &[u8], prefix: &str) -> Vec<u8> {
    if prefix.is_empty() {
        return body.to_vec();
    }
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let builtins: HashSet<String> = augment_claude_builtin_tool_registry(body, None)
        .into_keys()
        .collect();
    let mut changed = false;
    if let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            if tool
                .get("type")
                .is_some_and(|value| !json_text(value).unwrap_or_default().is_empty())
            {
                continue;
            }
            changed |= prefix_object_field(tool, "name", prefix, &builtins);
        }
    }
    if root.pointer("/tool_choice/type").and_then(Value::as_str) == Some("tool") {
        if let Some(choice) = root.get_mut("tool_choice") {
            changed |= prefix_object_field(choice, "name", prefix, &builtins);
        }
    }
    visit_message_tool_names(&mut root, |object, key| {
        changed |= prefix_object_field(object, key, prefix, &builtins);
    });
    if changed {
        encode_or_original(&root, body)
    } else {
        body.to_vec()
    }
}

pub fn strip_claude_tool_prefix_from_response(body: &[u8], prefix: &str) -> Vec<u8> {
    if prefix.is_empty() {
        return body.to_vec();
    }
    mutate_response_tool_names(body, |name| name.strip_prefix(prefix).map(str::to_owned))
}

pub fn strip_claude_tool_prefix_from_stream_line(line: &[u8], prefix: &str) -> Vec<u8> {
    if prefix.is_empty() {
        return line.to_vec();
    }
    mutate_stream_tool_name(line, |name| name.strip_prefix(prefix).map(str::to_owned))
}

pub fn restore_claude_oauth_tool_names_from_response(
    body: &[u8],
    prefix: &str,
    prefix_disabled: bool,
    reverse: &HashMap<String, String>,
) -> Vec<u8> {
    let body = if prefix_disabled {
        body.to_vec()
    } else {
        strip_claude_tool_prefix_from_response(body, prefix)
    };
    reverse_remap_claude_oauth_tool_names(&body, reverse)
}

pub fn restore_claude_oauth_tool_names_from_stream_line(
    line: &[u8],
    prefix: &str,
    prefix_disabled: bool,
    reverse: &HashMap<String, String>,
) -> Vec<u8> {
    let line = if prefix_disabled {
        line.to_vec()
    } else {
        strip_claude_tool_prefix_from_stream_line(line, prefix)
    };
    reverse_remap_claude_oauth_tool_names_from_stream_line(&line, reverse)
}

fn rename_object_field(
    value: &mut Value,
    key: &str,
    mapping: &HashMap<&str, &str>,
    reverse: &mut HashMap<String, String>,
) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let Some(original) = object.get(key).and_then(Value::as_str).map(str::to_owned) else {
        return false;
    };
    let Some(renamed) = mapping.get(original.as_str()).copied() else {
        return false;
    };
    if renamed == original {
        return false;
    }
    object.insert(key.to_owned(), Value::String(renamed.to_owned()));
    reverse.entry(renamed.to_owned()).or_insert(original);
    true
}

fn prefix_object_field(
    value: &mut Value,
    key: &str,
    prefix: &str,
    builtins: &HashSet<String>,
) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let Some(name) = object.get(key).and_then(Value::as_str) else {
        return false;
    };
    if name.is_empty() || name.starts_with(prefix) || builtins.contains(name) {
        return false;
    }
    object.insert(key.to_owned(), Value::String(format!("{prefix}{name}")));
    true
}

fn visit_message_tool_names(root: &mut Value, mut visit: impl FnMut(&mut Value, &str)) {
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        let Some(parts) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        for part in parts {
            match part.get("type").and_then(Value::as_str) {
                Some("tool_use") => visit(part, "name"),
                Some("tool_reference") => visit(part, "tool_name"),
                Some("tool_result") => {
                    if let Some(nested) = part.get_mut("content").and_then(Value::as_array_mut) {
                        for child in nested {
                            if child.get("type").and_then(Value::as_str) == Some("tool_reference") {
                                visit(child, "tool_name");
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn mutate_response_tool_names(body: &[u8], mut map: impl FnMut(&str) -> Option<String>) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let mut changed = false;
    if let Some(parts) = root.get_mut("content").and_then(Value::as_array_mut) {
        for part in parts {
            match part.get("type").and_then(Value::as_str) {
                Some("tool_use") => changed |= replace_object_field(part, "name", &mut map),
                Some("tool_reference") => {
                    changed |= replace_object_field(part, "tool_name", &mut map)
                }
                Some("tool_result") => {
                    if let Some(nested) = part.get_mut("content").and_then(Value::as_array_mut) {
                        for child in nested {
                            if child.get("type").and_then(Value::as_str) == Some("tool_reference") {
                                changed |= replace_object_field(child, "tool_name", &mut map);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if changed {
        encode_or_original(&root, body)
    } else {
        body.to_vec()
    }
}

fn mutate_stream_tool_name(line: &[u8], mut map: impl FnMut(&str) -> Option<String>) -> Vec<u8> {
    let trimmed = trim_ascii(line);
    let (sse, payload) = trimmed
        .strip_prefix(b"data:")
        .map_or((false, trimmed), |payload| (true, trim_ascii(payload)));
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return line.to_vec();
    };
    let Some(block) = root.get_mut("content_block") else {
        return line.to_vec();
    };
    let changed = match block.get("type").and_then(Value::as_str) {
        Some("tool_use") => replace_object_field(block, "name", &mut map),
        Some("tool_reference") => replace_object_field(block, "tool_name", &mut map),
        _ => false,
    };
    if !changed {
        return line.to_vec();
    }
    let encoded = encode_or_original(&root, payload);
    if sse {
        [b"data: ".as_slice(), encoded.as_slice()].concat()
    } else {
        encoded
    }
}

fn replace_object_field(
    value: &mut Value,
    key: &str,
    map: &mut impl FnMut(&str) -> Option<String>,
) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let Some(current) = object.get(key).and_then(Value::as_str) else {
        return false;
    };
    let Some(replacement) = map(current) else {
        return false;
    };
    object.insert(key.to_owned(), Value::String(replacement));
    true
}

fn json_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Null => None,
        value => Some(value.to_string()),
    }
}

fn non_empty_trimmed(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn encode_or_original(value: &Value, original: &[u8]) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_else(|_| original.to_vec())
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

/// Chrome-133/BoringSSL Claude Messages transport.
///
/// This native transport remains feature-isolated from the embeddable CTOX
/// daemon dependency. Retry/refresh ownership belongs to the conductor, not
/// wreq.
#[derive(Clone)]
#[cfg(feature = "anthropic-fingerprint-transport")]
pub struct ClaudeMessagesHttpTransport {
    client: Client,
}

#[cfg(feature = "anthropic-fingerprint-transport")]
impl ClaudeMessagesHttpTransport {
    pub fn new(proxy_url: Option<&str>) -> Result<Self, AnthropicTransportBuildError> {
        Ok(Self {
            client: build_anthropic_messages_client(proxy_url)?,
        })
    }

    fn prepare_outgoing(
        &self,
        request: &ClaudeMessagesRequest,
        timeout: Duration,
    ) -> RequestBuilder {
        let authorization = request.authorization();
        let fingerprint = request.fingerprint();
        let device = fingerprint.device();
        let beta_header = if request.betas().is_empty() {
            claude_code_cli_betas(
                request.body(),
                &HashSet::new(),
                request.mode() == ClaudeCredentialMode::OAuth,
            )
        } else {
            merged_claude_beta_header(request.betas())
        };
        let mut outgoing = self
            .client
            .post(request.endpoint())
            .header(CONTENT_TYPE, "application/json")
            .header("Anthropic-Version", ANTHROPIC_VERSION)
            .header("Anthropic-Beta", beta_header)
            .header("X-App", "cli")
            .header("X-Stainless-Retry-Count", "0")
            .header("X-Stainless-Runtime", "node")
            .header("X-Stainless-Lang", "js")
            .header("X-Stainless-Timeout", CLAUDE_CODE_TIMEOUT_HEADER)
            .header("X-Claude-Code-Session-Id", fingerprint.session_id())
            .header(USER_AGENT, device.user_agent())
            .header("X-Stainless-Package-Version", device.package_version())
            .header("X-Stainless-Runtime-Version", device.runtime_version())
            .header("X-Stainless-Os", device.os())
            .header("X-Stainless-Arch", device.arch())
            .header(CONNECTION, "keep-alive")
            .header(
                authorization.set_header().as_str(),
                authorization.expose_header_value(),
            )
            .timeout(timeout);
        if let Some(client_request_id) = request.client_request_id_for_target() {
            outgoing = outgoing.header("x-client-request-id", client_request_id);
        }
        if authorization.set_header() == ClaudeAuthorizationHeader::XApiKey {
            outgoing = outgoing.header("Anthropic-Dangerous-Direct-Browser-Access", "true");
        }
        outgoing = if request.stream() {
            outgoing
                .header(ACCEPT, "text/event-stream")
                .header(ACCEPT_ENCODING, "identity")
        } else {
            outgoing
                .header(ACCEPT, "application/json")
                .header(ACCEPT_ENCODING, "gzip, deflate, br, zstd")
        };
        outgoing
            .orig_headers(claude_messages_orig_headers())
            .body(request.body().to_vec())
    }

    fn prepare_count_tokens_outgoing(
        &self,
        request: &ClaudeMessagesRequest,
        timeout: Duration,
    ) -> RequestBuilder {
        let authorization = request.authorization();
        let fingerprint = request.fingerprint();
        let device = fingerprint.device();
        let beta_header = merged_claude_count_tokens_beta_header(request);
        let mut outgoing = self
            .client
            .post(format!(
                "{}://{}/v1/messages/count_tokens?beta=true",
                request.target().scheme(),
                request.target().authority()
            ))
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .header(USER_AGENT, device.user_agent())
            .header("X-Claude-Code-Session-Id", fingerprint.session_id())
            .header("X-Stainless-Arch", device.arch())
            .header("X-Stainless-Lang", "js")
            .header("X-Stainless-Os", device.os())
            .header("X-Stainless-Package-Version", device.package_version())
            .header("X-Stainless-Retry-Count", "0")
            .header("X-Stainless-Runtime", "node")
            .header("X-Stainless-Runtime-Version", device.runtime_version())
            .header("Anthropic-Beta", beta_header)
            .header("Anthropic-Version", ANTHROPIC_VERSION)
            .header("X-App", "cli")
            .header(CONNECTION, "keep-alive")
            .header(ACCEPT_ENCODING, "gzip, deflate, br, zstd")
            .header(
                authorization.set_header().as_str(),
                authorization.expose_header_value(),
            )
            .timeout(timeout);
        if let Some(client_request_id) = request.client_request_id_for_target() {
            outgoing = outgoing.header("x-client-request-id", client_request_id);
        }
        if authorization.set_header() == ClaudeAuthorizationHeader::XApiKey {
            outgoing = outgoing.header("Anthropic-Dangerous-Direct-Browser-Access", "true");
        }
        outgoing
            .orig_headers(claude_count_tokens_orig_headers())
            .body(request.body().to_vec())
    }
}

#[cfg(feature = "anthropic-fingerprint-transport")]
pub(super) fn claude_messages_orig_headers() -> wreq::header::OrigHeaderMap {
    let mut headers =
        wreq::header::OrigHeaderMap::with_capacity(CLAUDE_CODE_MESSAGES_HEADER_ORDER.len());
    for name in CLAUDE_CODE_MESSAGES_HEADER_ORDER {
        headers.insert(name);
    }
    headers
}

#[cfg(feature = "anthropic-fingerprint-transport")]
pub(super) fn claude_count_tokens_orig_headers() -> wreq::header::OrigHeaderMap {
    let mut headers =
        wreq::header::OrigHeaderMap::with_capacity(CLAUDE_CODE_COUNT_TOKENS_HEADER_ORDER.len());
    for name in CLAUDE_CODE_COUNT_TOKENS_HEADER_ORDER {
        headers.insert(name);
    }
    headers
}

#[cfg(feature = "anthropic-fingerprint-transport")]
fn merged_claude_beta_header(extra: &[String]) -> String {
    let mut values = Vec::new();
    let mut seen = HashSet::new();
    for beta in extra {
        let beta = beta.trim();
        if !beta.is_empty() && seen.insert(beta.to_owned()) {
            values.push(beta.to_owned());
        }
    }
    values.join(",")
}

#[cfg(feature = "anthropic-fingerprint-transport")]
pub(super) fn merged_claude_count_tokens_beta_header(request: &ClaudeMessagesRequest) -> String {
    let mut values = claude_count_tokens_betas(request.mode() == ClaudeCredentialMode::OAuth)
        .split(',')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut seen = values.iter().cloned().collect::<HashSet<_>>();
    for beta in request.betas() {
        let beta = beta.trim();
        if !beta.is_empty() && seen.insert(beta.to_owned()) {
            values.push(beta.to_owned());
        }
    }
    values.join(",")
}

#[cfg(feature = "anthropic-fingerprint-transport")]
impl fmt::Debug for ClaudeMessagesHttpTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeMessagesHttpTransport")
            .field("client", &"Chrome133/BoringSSL")
            .finish()
    }
}

#[cfg(feature = "anthropic-fingerprint-transport")]
impl ClaudeMessagesTransport for ClaudeMessagesHttpTransport {
    fn execute<'a>(
        &'a self,
        request: &'a ClaudeMessagesRequest,
        timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let response = self
                .prepare_outgoing(request, timeout)
                .send()
                .await
                .map_err(classify_transport_error)?;
            let status = response.status().as_u16();
            let mut headers = claude_response_headers(response.headers());
            let retry_after = parse_retry_delay(
                response
                    .headers()
                    .get("Retry-After")
                    .and_then(|value| value.to_str().ok()),
                response
                    .headers()
                    .get("Retry-After-Ms")
                    .and_then(|value| value.to_str().ok()),
                SystemTime::now(),
            );
            let content_encoding = response
                .headers()
                .get("Content-Encoding")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let body = response
                .bytes()
                .await
                .map_err(classify_transport_error)?
                .to_vec();
            let encoded = claude_response_is_encoded(&body, content_encoding.as_deref());
            let body = decode_claude_response_body(&body, content_encoding.as_deref())
                .map_err(|error| ClaudeMessagesTransportFailure::ResponseDecode(error.encoding))?;
            if encoded {
                remove_representation_headers(&mut headers);
            }
            Ok(ClaudeMessagesResponse::new(status, body)
                .with_retry_after(retry_after)
                .with_headers(headers))
        })
    }

    fn execute_count_tokens<'a>(
        &'a self,
        request: &'a ClaudeMessagesRequest,
        timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let response = self
                .prepare_count_tokens_outgoing(request, timeout)
                .send()
                .await
                .map_err(classify_transport_error)?;
            let status = response.status().as_u16();
            let mut headers = claude_response_headers(response.headers());
            let retry_after = parse_retry_delay(
                response
                    .headers()
                    .get("Retry-After")
                    .and_then(|value| value.to_str().ok()),
                response
                    .headers()
                    .get("Retry-After-Ms")
                    .and_then(|value| value.to_str().ok()),
                SystemTime::now(),
            );
            let content_encoding = response
                .headers()
                .get("Content-Encoding")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let body = response
                .bytes()
                .await
                .map_err(classify_transport_error)?
                .to_vec();
            let encoded = claude_response_is_encoded(&body, content_encoding.as_deref());
            let body = decode_claude_response_body(&body, content_encoding.as_deref())
                .map_err(|error| ClaudeMessagesTransportFailure::ResponseDecode(error.encoding))?;
            if encoded {
                remove_representation_headers(&mut headers);
            }
            Ok(ClaudeMessagesResponse::new(status, body)
                .with_retry_after(retry_after)
                .with_headers(headers))
        })
    }
}

#[cfg(feature = "anthropic-fingerprint-transport")]
impl ClaudeMessagesStreamingTransport for ClaudeMessagesHttpTransport {
    fn execute_stream<'a>(
        &'a self,
        request: &'a ClaudeMessagesRequest,
        timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<ClaudeMessagesStreamResponse, ClaudeMessagesTransportFailure>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let response = self
                .prepare_outgoing(request, timeout)
                .send()
                .await
                .map_err(classify_transport_error)?;
            let status = response.status().as_u16();
            let mut headers = claude_response_headers(response.headers());
            let retry_after = parse_retry_delay(
                response
                    .headers()
                    .get("Retry-After")
                    .and_then(|value| value.to_str().ok()),
                response
                    .headers()
                    .get("Retry-After-Ms")
                    .and_then(|value| value.to_str().ok()),
                SystemTime::now(),
            );
            let content_encoding = response
                .headers()
                .get("Content-Encoding")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let (sender, receiver) = mpsc::channel(8);
            let mut error_body = Vec::new();
            if !(200..300).contains(&status) {
                let body = response
                    .bytes()
                    .await
                    .map_err(classify_transport_error)?
                    .to_vec();
                let encoded = claude_response_is_encoded(&body, content_encoding.as_deref());
                error_body = decode_claude_response_body(&body, content_encoding.as_deref())
                    .map_err(|error| {
                        ClaudeMessagesTransportFailure::ResponseDecode(error.encoding)
                    })?;
                if encoded {
                    remove_representation_headers(&mut headers);
                }
            } else {
                let mut bytes = response.bytes_stream();
                if content_encoding.is_some() {
                    remove_representation_headers(&mut headers);
                }
                tokio::spawn(async move {
                    let Some(first) = bytes.next().await else {
                        return;
                    };
                    let first = match first {
                        Ok(value) => value.to_vec(),
                        Err(error) => {
                            let _ = sender.send(Err(classify_transport_error(error))).await;
                            return;
                        }
                    };
                    let compressed =
                        claude_response_is_encoded(&first, content_encoding.as_deref());
                    if compressed {
                        let mut body = first;
                        while let Some(chunk) = bytes.next().await {
                            match chunk {
                                Ok(value) => body.extend_from_slice(&value),
                                Err(error) => {
                                    let _ = sender.send(Err(classify_transport_error(error))).await;
                                    return;
                                }
                            }
                        }
                        let decoded =
                            decode_claude_response_body(&body, content_encoding.as_deref())
                                .map_err(|error| {
                                    ClaudeMessagesTransportFailure::ResponseDecode(error.encoding)
                                });
                        let _ = sender.send(decoded).await;
                        return;
                    }
                    if sender.send(Ok(first)).await.is_err() {
                        return;
                    }
                    while let Some(chunk) = bytes.next().await {
                        match chunk {
                            Ok(value) => {
                                if sender.send(Ok(value.to_vec())).await.is_err() {
                                    return;
                                }
                            }
                            Err(error) => {
                                let _ = sender.send(Err(classify_transport_error(error))).await;
                                return;
                            }
                        }
                    }
                });
            }
            Ok(
                ClaudeMessagesStreamResponse::new(status, retry_after, receiver)
                    .with_headers(headers)
                    .with_error_body(error_body),
            )
        })
    }
}

#[cfg(feature = "anthropic-fingerprint-transport")]
fn claude_response_headers(headers: &wreq::header::HeaderMap) -> Headers {
    let mut copied = Headers::new();
    for name in headers.keys() {
        let values = headers
            .get_all(name)
            .iter()
            .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
            .collect::<Vec<_>>();
        copied.insert(name.as_str().to_owned(), values);
    }
    copied
}

#[cfg(feature = "anthropic-fingerprint-transport")]
fn remove_representation_headers(headers: &mut Headers) {
    headers.retain(|name, _| {
        !name.eq_ignore_ascii_case("content-encoding")
            && !name.eq_ignore_ascii_case("content-length")
    });
}

#[cfg(feature = "anthropic-fingerprint-transport")]
fn claude_response_is_encoded(body: &[u8], content_encoding: Option<&str>) -> bool {
    content_encoding.is_some_and(|value| {
        value.split(',').map(str::trim).any(|encoding| {
            matches!(
                encoding.to_ascii_lowercase().as_str(),
                "gzip" | "deflate" | "br" | "zstd"
            )
        })
    }) || body.starts_with(&[0x1f, 0x8b])
        || body.starts_with(&[0x28, 0xb5, 0x2f, 0xfd])
}

#[cfg(feature = "anthropic-fingerprint-transport")]
fn parse_retry_delay(
    retry_after: Option<&str>,
    retry_after_ms: Option<&str>,
    now: SystemTime,
) -> Option<Duration> {
    if let Some(raw) = retry_after.map(str::trim).filter(|value| !value.is_empty()) {
        if let Ok(seconds) = raw.parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
        if let Ok(when) = httpdate::parse_http_date(raw) {
            return Some(when.duration_since(now).unwrap_or(Duration::ZERO));
        }
    }
    retry_after_ms
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(Duration::from_millis)
}

#[cfg(feature = "anthropic-fingerprint-transport")]
fn classify_transport_error(error: wreq::Error) -> ClaudeMessagesTransportFailure {
    if error.is_timeout() {
        ClaudeMessagesTransportFailure::Timeout
    } else if error.is_connect() {
        ClaudeMessagesTransportFailure::Connect
    } else {
        ClaudeMessagesTransportFailure::Protocol
    }
}

#[cfg(test)]
mod payload_tests {
    use super::*;

    fn value(raw: &[u8]) -> Value {
        serde_json::from_slice(raw).expect("valid transformed JSON")
    }

    fn sse_value(raw: &[u8]) -> Value {
        let raw = trim_ascii(raw);
        let payload = raw.strip_prefix(b"data:").map(trim_ascii).unwrap_or(raw);
        value(payload)
    }

    #[test]
    fn extracts_betas_and_fast_mode_only_when_requested() {
        let (betas, body) =
            extract_and_remove_claude_betas(br#"{"speed":"FAST","betas":[" custom ","",7]}"#);
        assert_eq!(betas, ["custom", "7"]);
        assert!(value(&body).get("betas").is_none());
        assert_eq!(
            append_claude_fast_mode_beta(&body, betas.clone()),
            vec!["custom", "7", CLAUDE_FAST_MODE_BETA]
        );
        assert_eq!(
            append_claude_fast_mode_beta(br#"{"speed":"normal"}"#, betas.clone()),
            betas
        );
    }

    #[test]
    fn oauth_finalization_installs_and_signs_fallback_billing_after_rewrites() {
        let input = br#"{"model":"claude-opus-5","messages":[{"role":"user","content":"hello"}],"tools":[]}"#;
        let (body, _, _) = prepare_claude_upstream_body_with_identity(input, None, "secret", true);
        let root = value(&body);
        let billing = root["system"][0]["text"].as_str().unwrap();
        assert!(billing.starts_with("x-anthropic-billing-header: cc_version=2.1.280."));
        assert!(billing.contains("cc_entrypoint=cli; cch="));
        assert!(!billing.contains("cch=00000;"));
        assert_eq!(sign_anthropic_messages_body(&body), body);
    }

    #[test]
    fn forced_tool_choice_removes_all_thinking_controls() {
        let output = disable_claude_thinking_if_tool_choice_forced(
            br#"{"thinking":{"type":"adaptive"},"output_config":{"effort":"max"},"tool_choice":{"type":"any"}}"#,
        );
        let output = value(&output);
        assert!(output.get("thinking").is_none());
        assert!(output.get("output_config").is_none());
    }

    #[test]
    fn sampling_normalization_matches_upstream_matrix() {
        let output = normalize_claude_sampling_for_upstream(
            br#"{"temperature":0.2,"top_p":0.9,"top_k":40,"thinking":{"type":"adaptive"}}"#,
        );
        let output = value(&output);
        assert!(output.get("temperature").is_none());
        assert!(output.get("top_p").is_none());
        assert!(output.get("top_k").is_none());

        let output = normalize_claude_sampling_for_upstream(
            br#"{"temperature":0.2,"top_p":0.9,"top_k":40}"#,
        );
        assert_eq!(value(&output)["top_k"], 40);
    }

    #[test]
    fn web_search_removes_only_empty_domain_lists() {
        let output = sanitize_claude_web_search_domains(
            br#"{"tools":[{"type":"custom","blocked_domains":[]},{"type":"web_search_20250305","allowed_domains":["a.test"],"blocked_domains":[],"max_uses":8}]}"#,
        );
        let output = value(&output);
        assert!(output["tools"][0].get("blocked_domains").is_some());
        assert!(output["tools"][1].get("blocked_domains").is_none());
        assert_eq!(output["tools"][1]["allowed_domains"][0], "a.test");
        assert_eq!(output["tools"][1]["max_uses"], 8);
    }

    #[test]
    fn prefix_mutates_custom_and_nested_references_but_not_builtins() {
        let output = apply_claude_tool_prefix(
            br#"{"tools":[{"type":"web_search_20250305","name":"web_search"},{"name":"Read"}],"tool_choice":{"type":"tool","name":"web_search"},"messages":[{"role":"user","content":[{"type":"tool_use","name":"Read"},{"type":"tool_reference","tool_name":"computer"},{"type":"tool_result","content":[{"type":"tool_reference","tool_name":"nested"}]}]}]}"#,
            "proxy_",
        );
        let output = value(&output);
        assert_eq!(output["tools"][0]["name"], "web_search");
        assert_eq!(output["tools"][1]["name"], "proxy_Read");
        assert_eq!(output["tool_choice"]["name"], "web_search");
        assert_eq!(output["messages"][0]["content"][0]["name"], "proxy_Read");
        assert_eq!(output["messages"][0]["content"][1]["tool_name"], "computer");
        assert_eq!(
            output["messages"][0]["content"][2]["content"][0]["tool_name"],
            "proxy_nested"
        );
    }

    #[test]
    fn strip_prefix_handles_response_and_sse_tool_references() {
        let output = strip_claude_tool_prefix_from_response(
            br#"{"content":[{"type":"tool_use","name":"proxy_Read"},{"type":"tool_result","content":[{"type":"tool_reference","tool_name":"proxy_nested"}]}]}"#,
            "proxy_",
        );
        let output = value(&output);
        assert_eq!(output["content"][0]["name"], "Read");
        assert_eq!(output["content"][1]["content"][0]["tool_name"], "nested");

        let line = strip_claude_tool_prefix_from_stream_line(
            br#"data: {"content_block":{"type":"tool_reference","tool_name":"proxy_Read"}}"#,
            "proxy_",
        );
        assert_eq!(sse_value(&line)["content_block"]["tool_name"], "Read");
    }

    #[test]
    fn oauth_reverse_map_only_records_actual_forward_renames() {
        let (output, reverse) = remap_claude_oauth_tool_names(
            br#"{"tools":[{"name":"Bash"},{"name":"glob"}],"messages":[{"content":[{"type":"tool_use","name":"glob"}]}]}"#,
        );
        let output = value(&output);
        assert_eq!(output["tools"][0]["name"], "Bash");
        assert_eq!(output["tools"][1]["name"], "Glob");
        assert_eq!(output["messages"][0]["content"][0]["name"], "Glob");
        assert_eq!(
            reverse,
            HashMap::from([("Glob".to_owned(), "glob".to_owned())])
        );

        let response = reverse_remap_claude_oauth_tool_names(
            br#"{"content":[{"type":"tool_use","name":"Bash"},{"type":"tool_use","name":"Glob"}]}"#,
            &reverse,
        );
        let response = value(&response);
        assert_eq!(response["content"][0]["name"], "Bash");
        assert_eq!(response["content"][1]["name"], "glob");
    }

    #[test]
    fn oauth_prefix_round_trip_preserves_client_casing() {
        let (request, reverse) = prepare_claude_oauth_tool_names_for_upstream(
            br#"{"tools":[{"name":"Bash"},{"name":"glob"}]}"#,
            "proxy_",
            false,
        );
        let request = value(&request);
        assert_eq!(request["tools"][0]["name"], "proxy_Bash");
        assert_eq!(request["tools"][1]["name"], "proxy_Glob");

        let response = restore_claude_oauth_tool_names_from_response(
            br#"{"content":[{"type":"tool_use","name":"proxy_Bash"},{"type":"tool_use","name":"proxy_Glob"}]}"#,
            "proxy_",
            false,
            &reverse,
        );
        let response = value(&response);
        assert_eq!(response["content"][0]["name"], "Bash");
        assert_eq!(response["content"][1]["name"], "glob");
    }

    #[test]
    fn oauth_stream_reverse_honors_request_local_map() {
        let reverse = HashMap::from([("Glob".to_owned(), "glob".to_owned())]);
        let bash = reverse_remap_claude_oauth_tool_names_from_stream_line(
            br#"data: {"content_block":{"type":"tool_use","name":"Bash"}}"#,
            &reverse,
        );
        assert_eq!(
            bash,
            br#"data: {"content_block":{"type":"tool_use","name":"Bash"}}"#
        );
        let glob = reverse_remap_claude_oauth_tool_names_from_stream_line(
            br#"data: {"content_block":{"type":"tool_use","name":"Glob"}}"#,
            &reverse,
        );
        assert_eq!(sse_value(&glob)["content_block"]["name"], "glob");
    }

    #[test]
    fn rebuilds_mid_system_messages_without_losing_existing_parts() {
        let output = rebuild_mid_claude_system_messages(
            br#"{"system":"top","messages":[{"role":"user","content":"one"},{"role":" SYSTEM ","content":["mid",{"type":"text","text":"two","cache_control":{"type":"ephemeral"}},{"type":"image"}]},{"role":"assistant","content":"ok"}]}"#,
        );
        let output = value(&output);
        assert_eq!(output["messages"].as_array().unwrap().len(), 2);
        assert_eq!(output["system"][0]["text"], "top");
        assert_eq!(output["system"][1]["text"], "mid");
        assert_eq!(output["system"][2]["text"], "two");
        assert_eq!(output["system"][2]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn max_tokens_is_typed_and_never_guessed_for_unknown_models() {
        use crate::internal::registry::ModelInfo;

        let info = ModelInfo {
            id: "test",
            provider_type: "claude",
            user_defined: true,
            max_completion_tokens: 4096,
            thinking: None,
        };
        let output = ensure_claude_model_max_tokens(br#"{"messages":[]}"#, Some(&info));
        assert_eq!(value(&output)["max_tokens"], 4096);
        let explicit =
            ensure_claude_model_max_tokens(br#"{"max_tokens":2048,"messages":[]}"#, Some(&info));
        assert_eq!(explicit, br#"{"max_tokens":2048,"messages":[]}"#);
        let unknown = ensure_claude_model_max_tokens(br#"{"messages":[]}"#, None);
        assert_eq!(unknown, br#"{"messages":[]}"#);
    }
}

#[cfg(all(test, feature = "anthropic-fingerprint-transport"))]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::SystemTime;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::internal::auth::claude::{
        ClaudeCredentialHandles, ClaudeRefreshCoordinator, ClaudeRefreshTransport,
        ClaudeSecretHandle, ClaudeSecretKind, ClaudeSecretStore, ClaudeStoredCredentials,
        RefreshClock, RefreshHttpResponse, RefreshRequest, RefreshTransportFailure,
        SecretStoreError, SecretString,
    };
    use crate::internal::runtime::executor::{
        AccountStateClock, ClaudeCredentialMode, ClaudeSubscriptionAccountPool,
        ClaudeSubscriptionAuth, ClaudeSubscriptionMessagesExecutor, ClaudeUpstreamTarget,
    };
    use crate::sdk::cliproxy::auth::{
        AccountCandidate, AccountRouter, CooldownConductor, CooldownStateRecord,
        CooldownStateStore, CooldownStoreError,
    };

    struct StaticSecretStore(Mutex<ClaudeStoredCredentials>);

    impl StaticSecretStore {
        fn new(access: &str, refresh: &str) -> Self {
            Self(Mutex::new(ClaudeStoredCredentials::new(
                SecretString::new(access).unwrap(),
                SecretString::new(refresh).unwrap(),
            )))
        }
    }

    impl ClaudeSecretStore for StaticSecretStore {
        fn load_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
        ) -> Result<ClaudeStoredCredentials, SecretStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn store_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
            credentials: &ClaudeStoredCredentials,
        ) -> Result<(), SecretStoreError> {
            *self.0.lock().unwrap() = credentials.clone();
            Ok(())
        }
    }

    struct UnusedRefreshTransport;

    impl ClaudeRefreshTransport for UnusedRefreshTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a RefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(RefreshTransportFailure::Protocol) })
        }
    }

    struct FixedClock;

    impl RefreshClock for FixedClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(10)
        }

        fn sleep(
            &self,
            _duration: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<(), RefreshTransportFailure>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    impl AccountStateClock for FixedClock {
        fn now_ms(&self) -> i64 {
            10_000
        }
    }

    #[derive(Default)]
    struct MemoryCooldownStore(Mutex<Vec<CooldownStateRecord>>);

    impl CooldownStateStore for MemoryCooldownStore {
        fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn save(&self, records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
            *self.0.lock().unwrap() = records.to_vec();
            Ok(())
        }
    }

    fn handles(prefix: &str) -> ClaudeCredentialHandles {
        ClaudeCredentialHandles::new(
            ClaudeSecretHandle::new(
                "subscriptions",
                format!("{prefix}-access"),
                ClaudeSecretKind::AccessToken,
            )
            .unwrap(),
            ClaudeSecretHandle::new(
                "subscriptions",
                format!("{prefix}-refresh"),
                ClaudeSecretKind::RefreshToken,
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn account_executor(
        auth_id: &str,
        access: &str,
        transport: ClaudeMessagesHttpTransport,
        conductor: Arc<CooldownConductor>,
    ) -> Arc<ClaudeSubscriptionMessagesExecutor> {
        let auth = Arc::new(ClaudeSubscriptionAuth::new(
            handles(auth_id),
            Arc::new(StaticSecretStore::new(access, "unused-refresh")),
            Arc::new(UnusedRefreshTransport),
            Arc::new(FixedClock),
            Arc::new(ClaudeRefreshCoordinator::default()),
        ));
        let stream_transport = transport.clone();
        Arc::new(
            ClaudeSubscriptionMessagesExecutor::new(
                auth,
                Arc::new(transport),
                Duration::from_secs(5),
            )
            .with_stream_transport(Arc::new(stream_transport))
            .with_account_state_clock(auth_id, conductor, Arc::new(FixedClock))
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn loopback_messages_request_applies_bearer_and_provider_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let authority = listener.local_addr().unwrap().to_string();
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_capture = Arc::clone(&captured);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if let Some(headers_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers_end = headers_end + 4;
                    let headers = String::from_utf8_lossy(&request[..headers_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= headers_end + content_length {
                        break;
                    }
                }
            }
            *server_capture.lock().await = request;
            let body = br#"{"type":"message","content":[]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nRetry-After: 7\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
        });

        let request = ClaudeMessagesRequest::new(
            ClaudeUpstreamTarget::new("http", authority).unwrap(),
            ClaudeCredentialMode::OAuth,
            &SecretString::new("oauth-do-not-leak").unwrap(),
            br#"{"model":"claude-sonnet-4-6","messages":[]}"#.to_vec(),
            false,
        )
        .unwrap();
        let response = ClaudeMessagesHttpTransport::new(None)
            .unwrap()
            .execute(&request, Duration::from_secs(5))
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(response.retry_after(), Some(Duration::from_secs(7)));
        assert_eq!(response.body(), br#"{"type":"message","content":[]}"#);
        let captured = String::from_utf8(captured.lock().await.clone()).unwrap();
        let lower = captured.to_ascii_lowercase();
        assert!(captured.starts_with("POST /v1/messages?beta=true HTTP/1.1\r\n"));
        assert!(lower.contains("authorization: bearer oauth-do-not-leak"));
        assert!(!lower.contains("x-api-key:"));
        assert!(lower.contains("anthropic-version: 2023-06-01"));
        assert!(lower.contains("anthropic-beta: claude-code-20250219"));
        assert!(lower.contains("x-app: cli"));
        assert!(lower.contains("x-claude-code-session-id: "));
        assert!(lower.contains("user-agent: claude-cli/2.1.280 (external, cli)"));
        assert!(lower.contains("x-stainless-package-version: 0.112.1"));
        assert!(lower.contains("x-stainless-runtime-version: v26.3.0"));
        assert!(lower.contains("x-stainless-os: macos"));
        assert!(lower.contains("x-stainless-arch: arm64"));
        assert!(lower.contains("x-stainless-timeout: 600"));
        assert!(lower.contains("content-type: application/json"));
        assert!(lower.contains("accept: application/json"));
        assert!(lower.contains("accept-encoding: gzip, deflate, br, zstd"));
        assert!(captured.contains("\r\n\r\n{\"model\":\"claude-sonnet-4-6\",\"messages\":[]}"));
    }

    #[tokio::test]
    async fn loopback_stream_bootstraps_before_upstream_tail_arrives() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let authority = listener.local_addr().unwrap().to_string();
        let (release_tail, wait_for_release) = tokio::sync::oneshot::channel();
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_capture = Arc::clone(&captured);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|part| part == b"\r\n\r\n") {
                    break;
                }
            }
            *server_capture.lock().await = request;
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            let first =
                b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_native\"}}\n\n";
            socket
                .write_all(format!("{:x}\r\n", first.len()).as_bytes())
                .await
                .unwrap();
            socket.write_all(first).await.unwrap();
            socket.write_all(b"\r\n").await.unwrap();
            wait_for_release.await.unwrap();
            let tail = b"data: {\"type\":\"message_stop\"}\n\n";
            socket
                .write_all(format!("{:x}\r\n", tail.len()).as_bytes())
                .await
                .unwrap();
            socket.write_all(tail).await.unwrap();
            socket.write_all(b"\r\n0\r\n\r\n").await.unwrap();
        });

        let request = ClaudeMessagesRequest::new(
            ClaudeUpstreamTarget::new("http", authority).unwrap(),
            ClaudeCredentialMode::OAuth,
            &SecretString::new("oauth-do-not-leak").unwrap(),
            br#"{"model":"claude-sonnet-4-6","messages":[],"stream":true}"#.to_vec(),
            true,
        )
        .unwrap();
        let transport = ClaudeMessagesHttpTransport::new(None).unwrap();
        let mut response = transport
            .execute_stream(&request, Duration::from_secs(5))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), response.bootstrap_message_start())
            .await
            .expect("bootstrap must not wait for the upstream tail")
            .unwrap();
        let first = response.next_chunk().await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&first).contains("message_start"));
        release_tail.send(()).unwrap();
        let tail = response.next_chunk().await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&tail).contains("message_stop"));
        server.await.unwrap();

        let captured = String::from_utf8(captured.lock().await.clone()).unwrap();
        let lower = captured.to_ascii_lowercase();
        assert!(lower.contains("accept: text/event-stream"));
        assert!(lower.contains("accept-encoding: identity"));
    }

    #[test]
    fn retry_after_ms_is_used_when_standard_header_is_absent() {
        assert_eq!(
            parse_retry_delay(None, Some("2500"), SystemTime::UNIX_EPOCH),
            Some(Duration::from_millis(2_500))
        );
    }

    #[tokio::test]
    async fn loopback_pool_persists_429_then_executes_with_second_account() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let authority = listener.local_addr().unwrap().to_string();
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::<Vec<u8>>::new()));
        let server_capture = Arc::clone(&captured);
        let server = tokio::spawn(async move {
            for (status, retry_after) in [(429, Some("7")), (200, None)] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let read = socket.read(&mut buffer).await.unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    if let Some(headers_end) =
                        request.windows(4).position(|part| part == b"\r\n\r\n")
                    {
                        let headers_end = headers_end + 4;
                        let headers = String::from_utf8_lossy(&request[..headers_end]);
                        let content_length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|value| value.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if request.len() >= headers_end + content_length {
                            break;
                        }
                    }
                }
                server_capture.lock().await.push(request);
                let body = if status == 200 {
                    br#"{"type":"message","content":[]}"#.as_slice()
                } else {
                    br#"{"type":"error"}"#.as_slice()
                };
                let retry_header = retry_after
                    .map(|value| format!("Retry-After: {value}\r\n"))
                    .unwrap_or_default();
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\n{retry_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.write_all(body).await.unwrap();
            }
        });

        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let transport = ClaudeMessagesHttpTransport::new(None).unwrap();
        let executors = HashMap::from([
            (
                "account-a".to_owned(),
                account_executor(
                    "account-a",
                    "access-a",
                    transport.clone(),
                    Arc::clone(&conductor),
                ),
            ),
            (
                "account-b".to_owned(),
                account_executor("account-b", "access-b", transport, Arc::clone(&conductor)),
            ),
        ]);
        let candidates = ["account-a", "account-b"]
            .into_iter()
            .map(|auth_id| AccountCandidate {
                auth_id: auth_id.to_owned(),
                provider: "claude".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            })
            .collect();
        let router = Arc::new(AccountRouter::new(cooldowns.clone()));
        let pool = ClaudeSubscriptionAccountPool::with_clock(
            router,
            candidates,
            executors,
            Arc::new(FixedClock),
        )
        .unwrap();

        let outcome = pool
            .execute(
                ClaudeUpstreamTarget::new("http", authority).unwrap(),
                "claude-sonnet-4-6",
                br#"{"model":"claude-sonnet-4-6","messages":[]}"#.to_vec(),
                false,
            )
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(outcome.selected_auth_id(), "account-b");
        assert_eq!(outcome.attempted_auth_ids(), ["account-a", "account-b"]);
        assert_eq!(outcome.outcome().response().status(), 200);
        let captured = captured.lock().await;
        assert_eq!(captured.len(), 2);
        assert!(String::from_utf8_lossy(&captured[0])
            .to_ascii_lowercase()
            .contains("authorization: bearer access-a"));
        assert!(String::from_utf8_lossy(&captured[1])
            .to_ascii_lowercase()
            .contains("authorization: bearer access-b"));
        let records = cooldowns.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].auth_id, "account-a");
        assert_eq!(records[0].next_retry_after_ms, Some(17_000));
    }
}
