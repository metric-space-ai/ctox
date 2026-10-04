// ref: internal/runtime/executor/devin_executor.go:1486-2430
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::helps::devin_request::{
    is_devin_codex_app_automation_update, sanitize_devin_tool_description, DevinImage, DevinPrompt,
    DevinTool, DevinToolCall, DEVIN_DEFAULT_MAX_TOKENS,
};
use crate::internal::signature::{detect_signature_provider, SignatureProvider};
use base64::{
    alphabet,
    engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig},
    Engine as _,
};
use gjson::{Kind, Value};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct DevinPreparedHistory {
    pub system_prompt: String,
    pub prompts: Vec<DevinPrompt>,
    pub tools: Vec<DevinTool>,
    pub temperature: Option<f64>,
    pub max_tokens: i64,
    pub session_id: String,
    pub cascade_id: String,
    pub thinking_level: String,
    pub budget_tokens: i64,
}
impl fmt::Debug for DevinPreparedHistory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinPreparedHistory")
            .field("prompt_count", &self.prompts.len())
            .field("tool_count", &self.tools.len())
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

fn string(value: &Value<'_>, path: &str) -> String {
    let field = value.get(path);
    field.str().to_owned()
}
fn first_nonempty(values: impl IntoIterator<Item = String>) -> String {
    values
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}
fn normalized(value: &Value<'_>, path: &str) -> String {
    string(value, path).trim().to_lowercase()
}
fn go_eq_fold_ascii(value: &str, expected: &str) -> bool {
    let mut left = value.chars();
    expected.bytes().all(|right| {
        left.next().is_some_and(|ch| {
            let folded = match ch {
                'ſ' => 's',
                'K' => 'k',
                _ => ch.to_ascii_lowercase(),
            };
            folded == char::from(right.to_ascii_lowercase())
        })
    }) && left.next().is_none()
}
fn prompt(source: i64) -> DevinPrompt {
    DevinPrompt {
        source,
        message_id: Uuid::new_v4().to_string(),
        ..Default::default()
    }
}
fn pending_result(pending: &mut Vec<String>, id: &str) -> Option<String> {
    let index = if id.is_empty() {
        (!pending.is_empty()).then_some(0)
    } else {
        pending.iter().position(|value| value == id)
    };
    index.map(|index| pending.remove(index))
}
fn result_prompt(step: &Value<'_>, id: String, pending: &mut Vec<String>) -> DevinPrompt {
    let (content, images) = extract_function_result_content(step);
    if let Some(matched) = pending_result(pending, &id) {
        let mut item = prompt(4);
        item.tool_call_id = if id.is_empty() { matched } else { id };
        item.content = content;
        item.images = images;
        item
    } else {
        let mut item = prompt(1);
        item.original_tool_call_id = id;
        item.is_orphaned_tool = true;
        item.content = content;
        item.images = images;
        item
    }
}
fn raw_or_string(value: &Value<'_>) -> String {
    if value.kind() == Kind::String {
        value.str().to_owned()
    } else if value.exists() {
        value.json().to_owned()
    } else {
        String::new()
    }
}
fn stable_session(root: &Value<'_>) -> String {
    first_nonempty(
        [
            "session_id",
            "sessionId",
            "conversation_id",
            "previous_interaction_id",
        ]
        .into_iter()
        .map(|path| string(root, path)),
    )
    .trim()
    .to_owned()
}

/// Owns all replay material. No request/credential content escapes through Debug,
/// mutable globals, environment configuration, or an original request borrow.
pub fn parse_devin_interactions_payload(
    payload: &[u8],
    original_request: &[u8],
) -> DevinPreparedHistory {
    let payload = String::from_utf8_lossy(payload);
    let original = String::from_utf8_lossy(original_request);
    let root = gjson::parse(&payload);
    let original_root = gjson::parse(&original);
    let mut result = DevinPreparedHistory::default();
    result.system_prompt = string(&root, "system_instruction").trim().to_owned();
    if result.system_prompt.is_empty() {
        result.system_prompt = string(&root, "systemInstruction").trim().to_owned();
    }
    let mut config = root.get("generation_config");
    if !config.exists() {
        config = root.get("generationConfig");
    }
    if config.exists() {
        let temperature = config.get("temperature");
        if temperature.exists() {
            result.temperature = Some(temperature.f64());
        }
        let maximum = config.get("max_output_tokens");
        result.max_tokens = maximum.i64();
        result.thinking_level = string(&config, "thinking_level");
        let budget = config.get("thinking_config.thinking_budget");
        result.budget_tokens = budget.i64();
    }
    if result.temperature.is_none() {
        let original_temperature = original_root.get("temperature");
        let temperature = root.get("temperature");
        if original_temperature.exists() {
            result.temperature = Some(original_temperature.f64());
        } else if temperature.exists() {
            result.temperature = Some(temperature.f64());
        }
    }
    if result.max_tokens <= 0 {
        result.max_tokens = DEVIN_DEFAULT_MAX_TOKENS;
    }
    result.session_id = stable_session(&root);
    if result.session_id.is_empty() && !original_request.is_empty() {
        result.session_id = stable_session(&original_root);
    }
    result.cascade_id = result.session_id.clone();
    let mut pending = Vec::new();
    let input = root.get("input");
    let messages = root.get("messages");
    if input.kind() == Kind::Array {
        for step in input.array() {
            match normalized(&step, "type").as_str() {
                "user_input" => {
                    let (content, images) = extract_interactions_step_content(&step);
                    let mut item = prompt(1);
                    item.content = content;
                    item.images = images;
                    result.prompts.push(item);
                }
                "model_output" | "thought" => {
                    let thinking = normalized(&step, "type") == "thought";
                    let content = extract_interactions_step_text(&step);
                    let sig = first_nonempty([
                        string(&step, "signature"),
                        string(&step, "thought_signature"),
                    ]);
                    let (signature, signature_type) = parse_devin_signature_bytes(&sig);
                    if result.prompts.last().is_none_or(|item| item.source != 2) {
                        result.prompts.push(prompt(2));
                    }
                    let last = result.prompts.last_mut().unwrap();
                    let text = if thinking {
                        &mut last.thinking
                    } else {
                        &mut last.content
                    };
                    if !text.is_empty() {
                        text.push_str(if thinking { "\n\n" } else { "\n" });
                    }
                    text.push_str(&content);
                    if last.signature.is_empty() && !signature.is_empty() {
                        last.signature = signature;
                        last.signature_type = signature_type;
                    }
                }
                "function_call" => {
                    let id = first_nonempty([string(&step, "id"), string(&step, "call_id")]);
                    let arguments = step.get("arguments");
                    let call = DevinToolCall {
                        id: id.clone(),
                        name: string(&step, "name"),
                        arguments: raw_or_string(&arguments),
                    };
                    if result.prompts.last().is_none_or(|item| item.source != 2) {
                        result.prompts.push(prompt(2));
                    }
                    result.prompts.last_mut().unwrap().tool_calls.push(call);
                    pending.push(id);
                }
                "function_result" => {
                    let id = first_nonempty([string(&step, "call_id"), string(&step, "id")]);
                    result.prompts.push(result_prompt(&step, id, &mut pending));
                }
                _ => {}
            }
        }
    } else if messages.kind() == Kind::Array {
        for message in messages.array() {
            match normalized(&message, "role").as_str() {
                "system" | "developer" => {
                    if result.system_prompt.is_empty() {
                        result.system_prompt = string(&message, "content");
                    }
                }
                "user" => {
                    let (content, images) = extract_interactions_step_content(&message);
                    let mut item = prompt(1);
                    item.content = content;
                    item.images = images;
                    result.prompts.push(item);
                }
                "assistant" => {
                    let mut item = prompt(2);
                    item.content = extract_interactions_step_text(&message);
                    let calls = message.get("tool_calls");
                    for call in calls.array() {
                        let id = first_nonempty([string(&call, "id"), string(&call, "call_id")]);
                        let name =
                            first_nonempty([string(&call, "function.name"), string(&call, "name")]);
                        let mut arguments = call.get("function.arguments");
                        if !arguments.exists() {
                            arguments = call.get("arguments");
                        }
                        item.tool_calls.push(DevinToolCall {
                            id: id.clone(),
                            name,
                            arguments: raw_or_string(&arguments),
                        });
                        pending.push(id);
                    }
                    result.prompts.push(item);
                }
                "tool" => {
                    let id = first_nonempty([
                        string(&message, "tool_call_id"),
                        string(&message, "id"),
                        string(&message, "call_id"),
                    ]);
                    result
                        .prompts
                        .push(result_prompt(&message, id, &mut pending));
                }
                _ => {}
            }
        }
    }
    if !original_request.is_empty() {
        supplement_signatures(&original_root, &mut result.prompts);
        supplement_images(&original_root, &mut result.prompts);
    }
    let tools = root.get("tools");
    for tool in tools.array() {
        if string(&tool, "type") == "namespace"
            && go_eq_fold_ascii(string(&tool, "name").trim(), "mcp__codex_app")
        {
            let mut children = tool.get("tools");
            if children.kind() != Kind::Array {
                children = tool.get("children");
            }
            for child in children.array() {
                if !go_eq_fold_ascii(string(&child, "name").trim(), "automation_update") {
                    append_tool(&child, &mut result.tools);
                }
            }
            continue;
        }
        let declarations = tool.get("function_declarations");
        if declarations.kind() == Kind::Array {
            for declaration in declarations.array() {
                append_tool(&declaration, &mut result.tools);
            }
            continue;
        }
        let declarations = tool.get("functionDeclarations");
        if declarations.kind() == Kind::Array {
            for declaration in declarations.array() {
                append_tool(&declaration, &mut result.tools);
            }
            continue;
        }
        append_tool(&tool, &mut result.tools);
    }
    result
}
fn append_tool(value: &Value<'_>, tools: &mut Vec<DevinTool>) {
    let name = string(value, "name");
    if name.is_empty() || is_devin_codex_app_automation_update("", &name) {
        return;
    }
    let description = sanitize_devin_tool_description(&name, &string(value, "description"));
    let mut parameters = value.get("parameters");
    if parameters.json().is_empty() {
        parameters = value.get("parametersJsonSchema");
    }
    tools.push(DevinTool {
        name,
        description,
        parameters: parameters.json().as_bytes().to_vec(),
    });
}

fn parse_data_url(raw: &str) -> Option<(String, String)> {
    let raw = raw.trim().strip_prefix("data:")?;
    let (header, data) = raw.split_once(',')?;
    let mime = header.split(';').next().unwrap_or_default().trim();
    Some((
        if mime.is_empty() { "image/png" } else { mime }.to_owned(),
        data.to_owned(),
    ))
}
fn mime_extension(mime: &str) -> &'static str {
    match mime.trim().to_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "png",
    }
}
fn extract_devin_image(part: &Value<'_>) -> Option<DevinImage> {
    if !matches!(
        normalized(part, "type").as_str(),
        "image" | "input_image" | "image_url"
    ) {
        return None;
    }
    let mut data = string(part, "data").trim().to_owned();
    let mut mime = string(part, "mime_type").trim().to_owned();
    if data.is_empty() {
        data = string(part, "source.data").trim().to_owned();
        if mime.is_empty() {
            mime = string(part, "source.media_type").trim().to_owned();
        }
    }
    if data.is_empty() {
        let url = first_nonempty([
            string(part, "image_url.url"),
            string(part, "image_url"),
            string(part, "url"),
        ]);
        if let Some((data_mime, bytes)) = parse_data_url(&url) {
            data = bytes;
            if mime.is_empty() {
                mime = data_mime;
            }
        }
    }
    if data.is_empty() {
        data = string(part, "inline_data.data").trim().to_owned();
        if mime.is_empty() {
            mime = string(part, "inline_data.mime_type").trim().to_owned();
        }
    }
    if data.is_empty() {
        return None;
    }
    if mime.is_empty() {
        mime = "image/png".to_owned();
    }
    Some(DevinImage {
        base64_data: data,
        mime_type: mime,
    })
}
fn prepend_image_headers(text: &mut String, images: &[DevinImage]) {
    if images.is_empty() || text.contains("[Image ") {
        return;
    }
    let header = images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            format!(
                "[Image {}: pasted_image_{}.{}]",
                index + 1,
                index + 1,
                mime_extension(&image.mime_type),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    *text = if text.is_empty() {
        header
    } else {
        format!("{header}\n\n{text}")
    };
}
fn extract_interactions_step_content(step: &Value<'_>) -> (String, Vec<DevinImage>) {
    let content = step.get("content");
    let mut texts = Vec::new();
    let mut images = Vec::new();
    match content.kind() {
        Kind::String => texts.push(content.str().to_owned()),
        Kind::Array => {
            for part in content.array() {
                if let Some(image) = extract_devin_image(&part) {
                    images.push(image);
                } else {
                    let text = string(&part, "text");
                    if !text.is_empty() {
                        texts.push(text);
                    }
                }
            }
        }
        _ => {
            let text = step.get("text");
            if text.exists() {
                texts.push(text.str().to_owned());
            }
        }
    }
    let mut text = texts.join("\n");
    prepend_image_headers(&mut text, &images);
    (text, images)
}
fn extract_interactions_step_text(step: &Value<'_>) -> String {
    let content = step.get("content");
    match content.kind() {
        Kind::String => content.str().to_owned(),
        Kind::Array => content
            .array()
            .iter()
            .map(|part| string(part, "text"))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => string(step, "text"),
    }
}
fn keys_allowed(value: &Value<'_>, allowed: &[&str]) -> bool {
    let mut valid = true;
    value.each(|key, _| {
        if !allowed.contains(&key.str()) {
            valid = false;
        }
        valid
    });
    valid
}
fn protocol_wrapper(value: &Value<'_>, key: &str) -> bool {
    if value.kind() != Kind::Object || !value.get(key).exists() {
        return false;
    }
    if go_eq_fold_ascii(string(value, "type").trim(), "tool_result") {
        keys_allowed(
            value,
            &[
                key,
                "type",
                "tool_use_id",
                "id",
                "is_error",
                "cache_control",
            ],
        )
    } else {
        keys_allowed(value, &[key, "cache_control"])
    }
}
fn extract_function_result_target(target: &Value<'_>) -> (String, Vec<DevinImage>) {
    if !target.exists() {
        return (String::new(), Vec::new());
    }
    if target.kind() == Kind::String {
        return (target.str().to_owned(), Vec::new());
    }
    if let Some(image) = extract_devin_image(target) {
        return (String::new(), vec![image]);
    }
    if target.kind() == Kind::Object {
        for key in ["content", "output", "result"] {
            if protocol_wrapper(target, key) {
                let nested = target.get(key);
                return extract_function_result_target(&nested);
            }
        }
        if normalized(target, "type") == "text"
            && keys_allowed(target, &["type", "text", "cache_control"])
        {
            return (string(target, "text"), Vec::new());
        }
        return (target.json().to_owned(), Vec::new());
    }
    if target.kind() == Kind::Array {
        let mut text_parts = Vec::new();
        let mut images = Vec::new();
        let mut structured = false;
        for item in target.array() {
            if let Some(image) = extract_devin_image(&item) {
                images.push(image);
                structured = true;
                continue;
            }
            let mut consumed = false;
            if item.kind() == Kind::Object {
                for key in ["content", "output", "result"] {
                    if protocol_wrapper(&item, key) {
                        structured = true;
                        let nested = item.get(key);
                        let (text, nested_images) = extract_function_result_target(&nested);
                        if !text.is_empty() {
                            text_parts.push(text);
                        }
                        images.extend(nested_images);
                        consumed = true;
                        break;
                    }
                }
                if consumed {
                    continue;
                }
                if normalized(&item, "type") == "text"
                    && keys_allowed(&item, &["type", "text", "cache_control"])
                {
                    structured = true;
                    let text = string(&item, "text");
                    if !text.is_empty() {
                        text_parts.push(text);
                    }
                    continue;
                }
            }
            let raw = item.json().trim();
            if !raw.is_empty() {
                text_parts.push(raw.to_owned());
            }
        }
        return if structured || !images.is_empty() {
            (text_parts.join("\n"), images)
        } else {
            (target.json().to_owned(), Vec::new())
        };
    }
    (target.json().to_owned(), Vec::new())
}
fn extract_function_result_content(step: &Value<'_>) -> (String, Vec<DevinImage>) {
    let mut target = step.get("result");
    if !target.exists() {
        target = step.get("output");
    }
    if !target.exists() {
        target = step.get("content");
    }
    if !target.exists() {
        return ("{}".to_owned(), Vec::new());
    }
    let (mut text, images) = extract_function_result_target(&target);
    prepend_image_headers(&mut text, &images);
    if text.trim().is_empty() && images.is_empty() {
        text = "{}".to_owned();
    }
    (text, images)
}
fn supplement_images(original: &Value<'_>, prompts: &mut [DevinPrompt]) {
    let messages = original.get("messages");
    if messages.kind() != Kind::Array {
        return;
    }
    let mut user_images = Vec::new();
    let mut tool_images: HashMap<String, Vec<DevinImage>> = HashMap::new();
    for message in messages.array() {
        let role = normalized(&message, "role");
        let content = message.get("content");
        match role.as_str() {
            "user" => {
                let mut images = Vec::new();
                for part in content.array() {
                    if normalized(&part, "type") == "tool_result" {
                        let id =
                            first_nonempty([string(&part, "tool_use_id"), string(&part, "id")]);
                        let tool_content = part.get("content");
                        let found = if tool_content.kind() == Kind::Array {
                            tool_content
                                .array()
                                .iter()
                                .filter_map(extract_devin_image)
                                .collect::<Vec<_>>()
                        } else {
                            extract_devin_image(&part).into_iter().collect()
                        };
                        if !id.is_empty() && !found.is_empty() {
                            tool_images.entry(id).or_default().extend(found);
                        }
                    } else if let Some(image) = extract_devin_image(&part) {
                        images.push(image);
                    }
                }
                user_images.push(images);
            }
            "tool" => {
                let id = first_nonempty([string(&message, "tool_call_id"), string(&message, "id")]);
                let images = if content.kind() == Kind::Array {
                    content
                        .array()
                        .iter()
                        .filter_map(extract_devin_image)
                        .collect::<Vec<_>>()
                } else {
                    extract_devin_image(&message).into_iter().collect()
                };
                if !id.is_empty() && !images.is_empty() {
                    tool_images.entry(id).or_default().extend(images);
                }
            }
            _ => {}
        }
    }
    let mut user_index = 0;
    for item in prompts {
        let matched = match item.source {
            1 if item.is_orphaned_tool => tool_images.get(&item.original_tool_call_id),
            1 => {
                let images = user_images.get(user_index);
                user_index += 1;
                images
            }
            4 => tool_images.get(&item.tool_call_id),
            _ => None,
        };
        if item.images.is_empty() {
            if let Some(images) = matched.filter(|images| !images.is_empty()) {
                item.images = images.clone();
                prepend_image_headers(&mut item.content, &item.images);
            }
        }
    }
}
fn signature_provider_type(signature: &str) -> Option<&'static str> {
    match detect_signature_provider(signature) {
        SignatureProvider::Claude => Some("anthropic"),
        SignatureProvider::Gpt => Some("openai"),
        SignatureProvider::Gemini => Some("gemini"),
        _ => None,
    }
}
fn detect_signature_type(signature: &str) -> &'static str {
    let s = signature.trim();
    for (prefix, kind) in [
        ("sealed.v1.", "sealed"),
        ("claude#", "anthropic"),
        ("gpt#", "openai"),
        ("gemini#", "gemini"),
    ] {
        if s.starts_with(prefix) {
            return kind;
        }
    }
    if let Some(kind) = signature_provider_type(s) {
        return kind;
    }
    if s.starts_with("CAQS") || s.starts_with("CAIS") {
        "anthropic"
    } else if s.starts_with("gAAAA") {
        "openai"
    } else if s.starts_with("AY") {
        "gemini"
    } else {
        "sealed"
    }
}
pub fn parse_devin_signature_bytes(signature: &str) -> (Vec<u8>, String) {
    let s = signature.trim();
    if s.is_empty() {
        return (Vec::new(), String::new());
    }
    if s.starts_with("sealed.v1.") {
        return (s.as_bytes().to_vec(), "sealed".to_owned());
    }
    for (prefix, kind) in [
        ("claude#", "anthropic"),
        ("gpt#", "openai"),
        ("gemini#", "gemini"),
    ] {
        if let Some(payload) = s.strip_prefix(prefix) {
            return (payload.as_bytes().to_vec(), kind.to_owned());
        }
    }
    if let Some(kind) = signature_provider_type(s) {
        return (s.as_bytes().to_vec(), kind.to_owned());
    }
    if s.starts_with("AY") {
        return (s.as_bytes().to_vec(), "gemini".to_owned());
    }
    // Go StdEncoding permits CR/LF and unused trailing bits; retain that grammar.
    let encoded: Vec<u8> = s
        .bytes()
        .filter(|byte| !matches!(*byte, b'\r' | b'\n'))
        .collect();
    let engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    );
    if let Ok(decoded) = engine.decode(encoded) {
        if !decoded.is_empty() {
            let decoded_text = String::from_utf8_lossy(&decoded);
            if decoded_text.starts_with("sealed.v1.") {
                return (decoded, "sealed".to_owned());
            }
            if let Some(kind) = signature_provider_type(&decoded_text) {
                return if kind == "gemini" {
                    (s.as_bytes().to_vec(), kind.to_owned())
                } else {
                    (decoded, kind.to_owned())
                };
            }
            if decoded_text.starts_with("CAQS") || decoded_text.starts_with("CAIS") {
                return (decoded, "anthropic".to_owned());
            }
            if decoded_text.starts_with("gAAAA") {
                return (decoded, "openai".to_owned());
            }
            if decoded[0] == 0x01 {
                return (s.as_bytes().to_vec(), "gemini".to_owned());
            }
        }
    }
    (s.as_bytes().to_vec(), detect_signature_type(s).to_owned())
}
fn supplement_signatures(original: &Value<'_>, prompts: &mut [DevinPrompt]) {
    let messages = original.get("messages");
    if messages.kind() != Kind::Array {
        return;
    }
    let mut assistants = Vec::new();
    for message in messages.array() {
        if !go_eq_fold_ascii(&string(&message, "role"), "assistant") {
            continue;
        }
        let mut signature = Vec::new();
        let mut signature_type = String::new();
        let mut thinking = String::new();
        let content = message.get("content");
        for part in content.array() {
            if string(&part, "type") != "thinking" {
                continue;
            }
            let raw = string(&part, "signature");
            if !raw.is_empty() {
                let (bytes, kind) = parse_devin_signature_bytes(&raw);
                if !bytes.is_empty() {
                    signature = bytes;
                    signature_type = kind;
                }
            }
            let text = string(&part, "thinking");
            if !text.is_empty() {
                thinking = text;
            }
        }
        assistants.push((signature, signature_type, thinking));
    }
    let mut assistant_index = 0;
    for item in prompts {
        if item.source != 2 {
            continue;
        }
        if let Some((signature, signature_type, thinking)) = assistants.get(assistant_index) {
            if item.signature.is_empty() && !signature.is_empty() {
                item.signature = signature.clone();
                item.signature_type = signature_type.clone();
            }
            if item.thinking.is_empty() && !thinking.is_empty() {
                item.thinking = thinking.clone();
            }
            assistant_index += 1;
        }
    }
}
