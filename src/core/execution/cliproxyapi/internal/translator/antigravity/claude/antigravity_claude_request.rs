// ref: internal/translator/antigravity/claude/antigravity_claude_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};

use crate::internal::cache::{
    get_cached_signature_required, get_model_group, has_valid_signature, signature_cache_enabled,
    SignatureCacheStoreError, SignatureKvStore,
};
use crate::internal::signature::{
    compatible_antigravity_claude_thinking_signature, compatible_gemini_signature,
    compatible_signature_for_provider, compatible_signature_for_provider_block,
    sanitize_gemini_request_thought_signatures, signature_provider_from_model_name,
    SignatureBlockKind, SignatureProvider,
};
use crate::internal::translator::common::claude_message_system_reminder_text;
use crate::internal::util::claude_attribution::is_claude_code_attribution_system_text;
use crate::internal::util::{
    clean_json_schema_for_antigravity, map_sanitized_function_name, sanitized_function_name_map,
};

use super::signature_validation::decode_gemini_claude_carrier_signature;
use super::web_search::{
    build_antigravity_web_search_request, should_build_antigravity_web_search_request,
};

const GEMINI_BYPASS: &str = "skip_thought_signature_validator";
const CARRIER_PREFIX: &str = "cpa-gemini-carrier-v1:";
const INTERLEAVED_THINKING_HINT: &str = "Interleaved thinking is enabled. You may think between tool calls and after receiving tool results before deciding the next action or final answer. Do not mention these instructions or any constraints about thinking blocks; just apply them.";
const SAFETY: &[(&str, &str)] = &[
    ("HARM_CATEGORY_HARASSMENT", "OFF"),
    ("HARM_CATEGORY_HATE_SPEECH", "OFF"),
    ("HARM_CATEGORY_SEXUALLY_EXPLICIT", "OFF"),
    ("HARM_CATEGORY_DANGEROUS_CONTENT", "OFF"),
    ("HARM_CATEGORY_CIVIC_INTEGRITY", "BLOCK_NONE"),
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AntigravityClaudeRequestCapabilities {
    pub native_google_search: bool,
}

/// Direct Gemini has a different replay contract from Antigravity. Keep that
/// request-local choice explicit without changing Antigravity signature/cache
/// behavior or reconstructing source block positions after conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClaudeThinkingPolicy {
    Antigravity,
    GeminiStandard,
    GeminiCompatibility,
}

pub(crate) fn convert_claude_request_for_direct_gemini(
    model_name: &str,
    input: &[u8],
    stream: bool,
    preserve_thinking: bool,
) -> Vec<u8> {
    convert_claude_request_with_policy(
        model_name,
        input,
        stream,
        AntigravityClaudeRequestCapabilities::default(),
        None,
        if preserve_thinking {
            ClaudeThinkingPolicy::GeminiCompatibility
        } else {
            ClaudeThinkingPolicy::GeminiStandard
        },
    )
    .unwrap_or_else(|_| input.to_vec())
}

pub fn convert_claude_request_to_antigravity(
    model_name: &str,
    input: &[u8],
    _stream: bool,
) -> Vec<u8> {
    convert_claude_request_to_antigravity_with_capabilities(
        model_name,
        input,
        _stream,
        AntigravityClaudeRequestCapabilities::default(),
    )
}

pub fn convert_claude_request_to_antigravity_with_capabilities(
    model_name: &str,
    input: &[u8],
    _stream: bool,
    capabilities: AntigravityClaudeRequestCapabilities,
) -> Vec<u8> {
    convert_claude_request_to_antigravity_with_runtime(
        model_name,
        input,
        _stream,
        capabilities,
        None,
    )
    .unwrap_or_else(|_| input.to_vec())
}

/// Fallible request-time boundary used when CTOX injects a durable signature
/// store. Every thinking lookup reads that store directly; errors propagate
/// before provider dispatch and never degrade to a local-cache conversion.
pub fn convert_claude_request_to_antigravity_with_runtime(
    model_name: &str,
    input: &[u8],
    stream: bool,
    capabilities: AntigravityClaudeRequestCapabilities,
    signature_store: Option<&dyn SignatureKvStore>,
) -> Result<Vec<u8>, AntigravityClaudeRequestTranslationError> {
    convert_claude_request_with_policy(
        model_name,
        input,
        stream,
        capabilities,
        signature_store,
        ClaudeThinkingPolicy::Antigravity,
    )
}

fn convert_claude_request_with_policy(
    model_name: &str,
    input: &[u8],
    _stream: bool,
    capabilities: AntigravityClaudeRequestCapabilities,
    signature_store: Option<&dyn SignatureKvStore>,
    thinking_policy: ClaudeThinkingPolicy,
) -> Result<Vec<u8>, AntigravityClaudeRequestTranslationError> {
    let Ok(root) = serde_json::from_slice::<Value>(input) else {
        return Ok(input.to_vec());
    };
    if should_build_antigravity_web_search_request(capabilities.native_google_search, &root) {
        return Ok(
            serde_json::to_vec(&build_antigravity_web_search_request(model_name, &root))
                .unwrap_or_else(|_| input.to_vec()),
        );
    }
    let function_name_map = sanitized_function_name_map(input);
    let mut system_parts = system_parts(&root);
    let mut enable_thought_translate = true;
    let contents = convert_messages(
        model_name,
        &root,
        &function_name_map,
        &mut enable_thought_translate,
        signature_store,
        thinking_policy,
    )?;
    let tools = convert_tools(&root, &function_name_map);
    let has_tools = tools
        .as_ref()
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty());
    let thinking_type = root
        .pointer("/thinking/type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let has_thinking = matches!(thinking_type, "enabled" | "adaptive" | "auto");
    let lower_model = model_name.to_ascii_lowercase();
    if has_tools
        && has_thinking
        && lower_model.contains("claude")
        && lower_model.contains("thinking")
    {
        system_parts.push(json!({"text":INTERLEAVED_THINKING_HINT}));
    }

    let mut request = Map::new();
    request.insert("contents".into(), Value::Array(contents));
    if !system_parts.is_empty() {
        request.insert("systemInstruction".into(), content("user", system_parts));
    }
    if let Some(tools) = tools {
        request.insert("tools".into(), tools);
    }
    apply_tool_choice(&root, &function_name_map, &mut request);
    apply_generation(&root, enable_thought_translate, thinking_type, &mut request);
    request.insert(
        "safetySettings".into(),
        Value::Array(
            SAFETY
                .iter()
                .map(|(category, threshold)| json!({"category":category,"threshold":threshold}))
                .collect(),
        ),
    );
    if thinking_policy == ClaudeThinkingPolicy::Antigravity
        && signature_provider_from_model_name(model_name) == SignatureProvider::Gemini
    {
        let sanitized = sanitize_gemini_request_thought_signatures(
            &serde_json::to_vec(&Value::Object(request.clone())).unwrap_or_default(),
        );
        if let Ok(Value::Object(sanitized)) = serde_json::from_slice(&sanitized) {
            request = sanitized;
        }
    }
    Ok(serde_json::to_vec(&json!({"model":model_name,"request":request})).unwrap_or_default())
}

/// Returns whether this Claude request is eligible for Antigravity's native
/// Google Search request shape. Account routing uses this before selection so
/// a request is never translated against a process-wide capability union.
pub fn claude_request_uses_native_web_search(input: &[u8]) -> bool {
    serde_json::from_slice::<Value>(input)
        .ok()
        .is_some_and(|root| should_build_antigravity_web_search_request(true, &root))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntigravityClaudeRequestTranslationError {
    SignatureCache(SignatureCacheStoreError),
}

impl std::fmt::Display for AntigravityClaudeRequestTranslationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Claude request translation dependency is unavailable")
    }
}

impl std::error::Error for AntigravityClaudeRequestTranslationError {}

impl From<SignatureCacheStoreError> for AntigravityClaudeRequestTranslationError {
    fn from(error: SignatureCacheStoreError) -> Self {
        Self::SignatureCache(error)
    }
}

fn system_parts(root: &Value) -> Vec<Value> {
    match root.get("system") {
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .filter(|text| !is_claude_code_attribution_system_text(text))
            .map(|text| {
                if text.is_empty() {
                    json!({})
                } else {
                    json!({"text":text})
                }
            })
            .collect(),
        Some(Value::String(text)) if !is_claude_code_attribution_system_text(text) => {
            vec![json!({"text":text})]
        }
        _ => Vec::new(),
    }
}

fn convert_messages(
    model_name: &str,
    root: &Value,
    function_name_map: &HashMap<String, String>,
    enable_thought_translate: &mut bool,
    signature_store: Option<&dyn SignatureKvStore>,
    thinking_policy: ClaudeThinkingPolicy,
) -> Result<Vec<Value>, AntigravityClaudeRequestTranslationError> {
    let mut output = Vec::new();
    let mut tool_name_by_id = HashMap::<String, String>::new();
    let Some(messages) = root.get("messages").and_then(Value::as_array) else {
        return Ok(output);
    };
    for message in messages {
        let Some(original_role) = message.get("role").and_then(Value::as_str) else {
            continue;
        };
        let role = match original_role {
            "assistant" => "model",
            "system" => "user",
            other => other,
        };
        let Some(message_content) = message.get("content") else {
            continue;
        };
        if original_role == "system" {
            if let Some(text) = claude_message_system_reminder_text(message_content) {
                output.push(content(role, vec![json!({"text":text})]));
            }
            continue;
        }
        if let Some(parts) = message_content.as_array() {
            let converted = convert_message_parts(
                model_name,
                original_role,
                parts,
                function_name_map,
                &mut tool_name_by_id,
                enable_thought_translate,
                signature_store,
                thinking_policy,
            )?;
            if thinking_policy == ClaudeThinkingPolicy::Antigravity {
                if !converted.is_empty() {
                    output.push(content(role, reorder_model_parts(role, converted)));
                }
            } else {
                // ref: gemini/claude/gemini_claude_request.go:202-215 @ e2bff010
                output.push(content(role, converted));
            }
        } else if let Some(text) = message_content.as_str() {
            output.push(content(
                role,
                vec![if text.is_empty() {
                    json!({})
                } else {
                    json!({"text":text})
                }],
            ));
        }
    }
    Ok(output)
}

fn convert_message_parts(
    model_name: &str,
    original_role: &str,
    source: &[Value],
    function_name_map: &HashMap<String, String>,
    tool_name_by_id: &mut HashMap<String, String>,
    enable_thought_translate: &mut bool,
    signature_store: Option<&dyn SignatureKvStore>,
    thinking_policy: ClaudeThinkingPolicy,
) -> Result<Vec<Value>, AntigravityClaudeRequestTranslationError> {
    let gemini = signature_provider_from_model_name(model_name) == SignatureProvider::Gemini;
    let mut parts = Vec::new();
    let mut pending_signature = String::new();
    let mut pending_target = String::new();
    for (index, item) in source.iter().enumerate() {
        match item.get("type").and_then(Value::as_str).unwrap_or_default() {
            "thinking" => {
                // ref: gemini/claude/gemini_claude_request.go:129-137 @ e2bff010
                if thinking_policy != ClaudeThinkingPolicy::Antigravity {
                    if thinking_policy == ClaudeThinkingPolicy::GeminiCompatibility {
                        let thinking = match item.get("thinking").unwrap_or(&Value::Null) {
                            Value::Null => String::new(),
                            Value::String(text) => text.clone(),
                            other => other.to_string(),
                        };
                        let raw_signature = item
                            .get("signature")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let signature = compatible_signature_for_provider_block(
                            SignatureProvider::Gemini,
                            raw_signature,
                            SignatureBlockKind::GeminiModelPart,
                        )
                        .unwrap_or_else(|| GEMINI_BYPASS.to_owned());
                        parts.push(json!({
                            "text": thinking,
                            "thought": true,
                            "thoughtSignature": signature
                        }));
                    }
                    continue;
                }
                if original_role != "assistant" {
                    continue;
                }
                let thinking = thinking_text(item);
                let raw_signature = item
                    .get("signature")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let decoded = decode_gemini_claude_carrier_signature(raw_signature);
                let marked = raw_signature.trim().starts_with(CARRIER_PREFIX);
                let mut signature = resolve_thinking_signature(
                    model_name,
                    &thinking,
                    raw_signature,
                    signature_store,
                )?;
                if !signature.is_empty() && !pending_signature.is_empty() {
                    if pending_signature != signature {
                        parts.push(detached_carrier(&pending_signature));
                    }
                    pending_signature.clear();
                    pending_target.clear();
                }
                let mut from_pending = false;
                if signature.is_empty()
                    && !thinking.is_empty()
                    && !pending_signature.is_empty()
                    && matches!(pending_target.as_str(), "" | "any" | "text")
                {
                    signature.clone_from(&pending_signature);
                    pending_signature.clear();
                    pending_target.clear();
                    from_pending = true;
                }
                if !has_resolved_thinking_signature(model_name, &signature) {
                    *enable_thought_translate = false;
                    continue;
                }
                let (next_accepts, next_target) = source
                    .get(index + 1)
                    .and_then(|next| match next.get("type").and_then(Value::as_str) {
                        Some("text") => Some((true, "text")),
                        Some("tool_use") => Some((true, "function")),
                        _ => None,
                    })
                    .unwrap_or((false, "any"));
                if !thinking.is_empty() {
                    let mut part = json!({"thought":true,"text":thinking});
                    if from_pending {
                        part["thoughtSignature"] = Value::String(signature);
                    } else if marked {
                        if let Some(carrier) = decoded {
                            if carrier.direction == "standalone"
                                && matches!(carrier.target_kind.as_str(), "text" | "any")
                            {
                                part["thoughtSignature"] = Value::String(signature);
                            } else if carrier.direction == "next"
                                && next_accepts
                                && (carrier.target_kind == "any"
                                    || carrier.target_kind == next_target)
                            {
                                pending_signature = signature;
                                pending_target = carrier.target_kind;
                            }
                        }
                    } else if gemini && next_accepts {
                        pending_signature = signature;
                        pending_target = next_target.to_owned();
                    } else {
                        part["thoughtSignature"] = Value::String(signature);
                    }
                    parts.push(part);
                    continue;
                }
                if !gemini || (marked && decoded.is_none()) {
                    continue;
                }
                if let Some(carrier) = decoded.as_ref().filter(|carrier| carrier.marked) {
                    if carrier.direction == "next" {
                        if carrier_matches_adjacent(source, index, "next", &carrier.target_kind) {
                            if !pending_signature.is_empty() {
                                parts.push(detached_carrier(&pending_signature));
                            }
                            pending_signature = signature;
                            pending_target.clone_from(&carrier.target_kind);
                        }
                        continue;
                    }
                    if carrier.direction == "standalone" {
                        parts.push(detached_carrier(&signature));
                        continue;
                    }
                }
                let bind_backward = decoded
                    .as_ref()
                    .is_some_and(|carrier| carrier.marked && carrier.direction == "previous");
                if bind_backward {
                    let carrier = decoded.as_ref().unwrap();
                    if !carrier_matches_adjacent(source, index, "previous", &carrier.target_kind) {
                        continue;
                    }
                } else if next_accepts {
                    if !pending_signature.is_empty() {
                        parts.push(detached_carrier(&pending_signature));
                    }
                    pending_signature = signature;
                    pending_target = next_target.to_owned();
                    continue;
                }
                let target = decoded
                    .as_ref()
                    .map(|carrier| carrier.target_kind.as_str())
                    .unwrap_or_default();
                let mut attached = false;
                let mut found_semantic = false;
                for part in parts.iter_mut().rev() {
                    let part_target = if part.get("functionCall").is_some() {
                        "function"
                    } else if part
                        .get("text")
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.is_empty())
                    {
                        "text"
                    } else {
                        continue;
                    };
                    found_semantic = true;
                    if marked && target != "any" && target != part_target {
                        break;
                    }
                    let current = part
                        .get("thoughtSignature")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if current.is_empty()
                        || (bind_backward && part_target == "function" && current == GEMINI_BYPASS)
                    {
                        part["thoughtSignature"] = Value::String(signature.clone());
                        attached = true;
                    }
                    break;
                }
                if !attached && (found_semantic || bind_backward) {
                    parts.push(detached_carrier(&signature));
                } else if !attached {
                    pending_signature = signature;
                    pending_target = target.to_owned();
                }
            }
            "text" => {
                let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                if text.is_empty() {
                    continue;
                }
                let mut part = json!({"text":text});
                if !pending_signature.is_empty() {
                    if matches!(pending_target.as_str(), "" | "any" | "text") {
                        part["thoughtSignature"] = Value::String(pending_signature.clone());
                    } else {
                        parts.push(detached_carrier(&pending_signature));
                    }
                    pending_signature.clear();
                    pending_target.clear();
                }
                parts.push(part);
            }
            "tool_use" => {
                let original_name = item.get("name").and_then(Value::as_str).unwrap_or_default();
                let name = map_sanitized_function_name(function_name_map, original_name);
                let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                if !id.is_empty() && !original_name.is_empty() {
                    tool_name_by_id.insert(id.to_owned(), original_name.to_owned());
                }
                let Some(args) = tool_input(item.get("input")) else {
                    continue;
                };
                let mut signature = tool_signature(model_name, item);
                if !pending_signature.is_empty() {
                    if matches!(pending_target.as_str(), "" | "any" | "function")
                        && (signature.is_empty() || signature == GEMINI_BYPASS)
                    {
                        signature.clone_from(&pending_signature);
                    } else {
                        parts.push(detached_carrier(&pending_signature));
                    }
                    pending_signature.clear();
                    pending_target.clear();
                }
                let mut function_call = json!({"name":name,"args":args});
                if !id.is_empty() {
                    function_call["id"] = Value::String(id.to_owned());
                }
                let mut part = json!({"functionCall":function_call});
                if !signature.is_empty() {
                    part["thoughtSignature"] = Value::String(signature);
                }
                parts.push(part);
            }
            "tool_result" => {
                if let Some(part) = tool_result_part(item, function_name_map, tool_name_by_id) {
                    parts.push(part);
                }
            }
            "image" => {
                if let Some(inline) = inline_image(item) {
                    parts.push(json!({"inlineData":inline}));
                }
            }
            _ => {}
        }
    }
    if !pending_signature.is_empty() {
        parts.push(detached_carrier(&pending_signature));
    }
    Ok(parts)
}

fn resolve_thinking_signature(
    model_name: &str,
    thinking_text: &str,
    raw: &str,
    signature_store: Option<&dyn SignatureKvStore>,
) -> Result<String, AntigravityClaudeRequestTranslationError> {
    let provider = signature_provider_from_model_name(model_name);
    if provider == SignatureProvider::Gemini {
        return Ok(decode_gemini_claude_carrier_signature(raw)
            .and_then(|carrier| compatible_gemini_signature(&carrier.signature))
            .unwrap_or_default());
    }
    if signature_cache_enabled() {
        if !thinking_text.is_empty() {
            let cached = get_cached_signature_required(signature_store, model_name, thinking_text)?;
            if !cached.is_empty() {
                return Ok(if provider == SignatureProvider::Claude {
                    compatible_antigravity_claude_thinking_signature(&cached).unwrap_or_default()
                } else {
                    cached
                });
            }
        }
        let client_signature = raw
            .split_once('#')
            .filter(|(group, _)| *group == get_model_group(model_name))
            .map(|(_, signature)| signature)
            .unwrap_or_default();
        if !has_valid_signature(model_name, client_signature) {
            return Ok(String::new());
        }
        return Ok(if provider == SignatureProvider::Claude {
            compatible_antigravity_claude_thinking_signature(client_signature).unwrap_or_default()
        } else {
            client_signature.to_owned()
        });
    }
    Ok(match provider {
        SignatureProvider::Claude => {
            compatible_antigravity_claude_thinking_signature(raw).unwrap_or_default()
        }
        other => compatible_signature_for_provider(other, raw).unwrap_or_default(),
    })
}

fn has_resolved_thinking_signature(model_name: &str, signature: &str) -> bool {
    match signature_provider_from_model_name(model_name) {
        SignatureProvider::Claude => {
            compatible_antigravity_claude_thinking_signature(signature).is_some()
        }
        SignatureProvider::Gemini => compatible_gemini_signature(signature).is_some(),
        provider => {
            compatible_signature_for_provider(provider, signature).is_some()
                || (signature_cache_enabled() && has_valid_signature(model_name, signature))
        }
    }
}

fn tool_signature(model_name: &str, item: &Value) -> String {
    let provider = signature_provider_from_model_name(model_name);
    for pointer in [
        "/signature",
        "/thought_signature",
        "/extra_content/google/thought_signature",
    ] {
        if let Some(raw) = item.pointer(pointer).and_then(Value::as_str) {
            let signature = match provider {
                SignatureProvider::Claude => compatible_antigravity_claude_thinking_signature(raw),
                SignatureProvider::Gemini => compatible_gemini_signature(raw),
                other => compatible_signature_for_provider(other, raw),
            };
            if let Some(signature) = signature {
                return signature;
            }
        }
    }
    if provider == SignatureProvider::Claude {
        String::new()
    } else {
        GEMINI_BYPASS.to_owned()
    }
}

fn tool_input(input: Option<&Value>) -> Option<Value> {
    match input {
        None => None,
        Some(Value::Null) => Some(json!({})),
        Some(Value::String(raw)) => serde_json::from_str::<Value>(raw)
            .ok()
            .filter(Value::is_object)
            .or_else(|| Some(Value::String(raw.clone()))),
        Some(value) => Some(value.clone()),
    }
}

fn tool_result_part(
    item: &Value,
    function_name_map: &HashMap<String, String>,
    tool_name_by_id: &HashMap<String, String>,
) -> Option<Value> {
    let id = item.get("tool_use_id").and_then(Value::as_str)?;
    if id.is_empty() {
        return None;
    }
    let original_name = tool_name_by_id.get(id).cloned().unwrap_or_else(|| {
        let segments = id.split('-').collect::<Vec<_>>();
        if segments.len() > 2 {
            segments[..segments.len() - 2].join("-")
        } else {
            id.to_owned()
        }
    });
    let mut response = json!({
        "id":id,
        "name":map_sanitized_function_name(function_name_map,&original_name),
        "response":{"result":""}
    });
    match item.get("content") {
        Some(Value::String(text)) => response["response"]["result"] = Value::String(text.clone()),
        Some(Value::Array(items)) => {
            let mut non_images = Vec::new();
            let mut images = Vec::new();
            for item in items {
                if let Some(inline) = inline_image(item) {
                    images.push(json!({"inlineData":inline}));
                } else {
                    non_images.push(item.clone());
                }
            }
            response["response"]["result"] = match non_images.len() {
                0 => Value::String(String::new()),
                1 => non_images.remove(0),
                _ => Value::Array(non_images),
            };
            if !images.is_empty() {
                response["parts"] = Value::Array(images);
            }
        }
        Some(value) if inline_image(value).is_some() => {
            response["parts"] =
                Value::Array(vec![json!({"inlineData":inline_image(value).unwrap()})]);
        }
        Some(value) => response["response"]["result"] = value.clone(),
        None => {}
    }
    Some(json!({"functionResponse":response}))
}

fn inline_image(item: &Value) -> Option<Value> {
    if item.get("type").and_then(Value::as_str) != Some("image")
        || item.pointer("/source/type").and_then(Value::as_str) != Some("base64")
    {
        return None;
    }
    let mut inline = Map::new();
    if let Some(mime) = item.pointer("/source/media_type").and_then(Value::as_str) {
        if !mime.is_empty() {
            inline.insert("mimeType".into(), Value::String(mime.to_owned()));
        }
    }
    if let Some(data) = item.pointer("/source/data").and_then(Value::as_str) {
        if !data.is_empty() {
            inline.insert("data".into(), Value::String(data.to_owned()));
        }
    }
    Some(Value::Object(inline))
}

fn convert_tools(root: &Value, name_map: &HashMap<String, String>) -> Option<Value> {
    let mut declarations = Vec::new();
    let mut seen = HashSet::new();
    for tool in root
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if matches!(
            tool.get("type").and_then(Value::as_str),
            Some("web_search_20250305" | "web_search_20260209")
        ) {
            continue;
        }
        let Some(schema) = tool.get("input_schema").filter(|schema| schema.is_object()) else {
            continue;
        };
        let mut declaration = Map::new();
        for key in [
            "name",
            "description",
            "behavior",
            "parameters",
            "parametersJsonSchema",
            "response",
            "responseJsonSchema",
        ] {
            if let Some(value) = tool.get(key) {
                declaration.insert(key.to_owned(), value.clone());
            }
        }
        let original = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        let mapped = map_sanitized_function_name(name_map, original);
        declaration.insert("name".into(), Value::String(mapped.clone()));
        declaration.insert(
            "parametersJsonSchema".into(),
            clean_json_schema_for_antigravity(schema),
        );
        declaration.remove("parameters");
        if mapped.is_empty() || !seen.insert(mapped) {
            continue;
        }
        declarations.push(Value::Object(declaration));
    }
    (!declarations.is_empty())
        .then(|| Value::Array(vec![json!({"functionDeclarations":declarations})]))
}

fn apply_tool_choice(
    root: &Value,
    name_map: &HashMap<String, String>,
    request: &mut Map<String, Value>,
) {
    let Some(choice) = root.get("tool_choice") else {
        return;
    };
    let (kind, name) = match choice {
        Value::String(kind) => (kind.as_str(), ""),
        Value::Object(choice) => (
            choice
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            choice
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        _ => return,
    };
    let mode = match kind {
        "auto" => "AUTO",
        "none" => "NONE",
        "any" | "tool" => "ANY",
        _ => return,
    };
    let mut config = json!({"functionCallingConfig":{"mode":mode}});
    if kind == "tool" && !name.is_empty() {
        config["functionCallingConfig"]["allowedFunctionNames"] =
            Value::Array(vec![Value::String(map_sanitized_function_name(
                name_map, name,
            ))]);
    }
    request.insert("toolConfig".into(), config);
}

fn apply_generation(
    root: &Value,
    enable_thought_translate: bool,
    thinking_type: &str,
    request: &mut Map<String, Value>,
) {
    let mut generation = Map::new();
    if enable_thought_translate {
        match thinking_type {
            "enabled" => {
                if root
                    .pointer("/thinking/budget_tokens")
                    .is_some_and(Value::is_number)
                {
                    generation.insert(
                        "thinkingConfig".into(),
                        json!({"thinkingBudget":root["thinking"]["budget_tokens"]}),
                    );
                }
            }
            "adaptive" | "auto" => {
                let effort = root
                    .pointer("/output_config/effort")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|effort| !effort.is_empty())
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_else(|| "high".to_owned());
                generation.insert("thinkingConfig".into(), json!({"thinkingLevel":effort}));
            }
            _ => {}
        }
    }
    for (source, target) in [
        ("temperature", "temperature"),
        ("top_p", "topP"),
        ("top_k", "topK"),
        ("max_tokens", "maxOutputTokens"),
    ] {
        if root.get(source).is_some_and(Value::is_number) {
            generation.insert(target.into(), root[source].clone());
        }
    }
    if !generation.is_empty() {
        request.insert("generationConfig".into(), Value::Object(generation));
    }
}

fn reorder_model_parts(role: &str, parts: Vec<Value>) -> Vec<Value> {
    if role != "model" || parts.len() < 2 {
        return parts;
    }
    let mut thinking = Vec::new();
    let mut regular = Vec::new();
    let mut trailing = Vec::new();
    let mut seen_function = false;
    for part in parts {
        let carrier = part.get("text").and_then(Value::as_str) == Some("")
            && part
                .get("thoughtSignature")
                .and_then(Value::as_str)
                .is_some_and(|signature| !signature.trim().is_empty());
        if part.get("thought").and_then(Value::as_bool) == Some(true) {
            thinking.push(part);
        } else if part.get("functionCall").is_some() || (carrier && seen_function) {
            seen_function |= part.get("functionCall").is_some();
            trailing.push(part);
        } else {
            regular.push(part);
        }
    }
    thinking.extend(regular);
    thinking.extend(trailing);
    thinking
}

fn content(role: &str, parts: Vec<Value>) -> Value {
    json!({"role":role,"parts":parts})
}

fn detached_carrier(signature: &str) -> Value {
    json!({"text":"","thoughtSignature":signature})
}

fn thinking_text(item: &Value) -> String {
    item.get("text")
        .and_then(Value::as_str)
        .or_else(|| item.get("thinking").and_then(Value::as_str))
        .or_else(|| item.pointer("/thinking/text").and_then(Value::as_str))
        .or_else(|| item.pointer("/thinking/thinking").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned()
}

fn semantic_target(item: &Value) -> Option<&'static str> {
    match item.get("type").and_then(Value::as_str) {
        Some("text") => Some("text"),
        Some("tool_use") => Some("function"),
        Some("thinking") if !thinking_text(item).trim().is_empty() => Some("text"),
        _ => None,
    }
}

fn carrier_matches_adjacent(source: &[Value], index: usize, direction: &str, target: &str) -> bool {
    let step = if direction == "previous" { -1 } else { 1 };
    let mut cursor = index as isize + step;
    while let Some(item) = usize::try_from(cursor)
        .ok()
        .filter(|cursor| *cursor < source.len())
        .and_then(|cursor| source.get(cursor))
    {
        if let Some(kind) = semantic_target(item) {
            return target == "any" || target == kind;
        }
        if item.get("type").and_then(Value::as_str) != Some("thinking")
            || !thinking_text(item).trim().is_empty()
        {
            return false;
        }
        cursor += step;
    }
    false
}

// Response-time durable cache publication and account-aware runtime assembly
// remain outside this pure request transformer.
