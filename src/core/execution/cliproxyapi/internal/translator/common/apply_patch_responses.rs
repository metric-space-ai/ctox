// ref: internal/translator/common/apply_patch_responses.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only

//! Adapts Responses `apply_patch` declarations and explicit custom history.
//!
//! Winners are collected before rewriting. `ApplyPatchResponsesBridge`
//! converts function-call events for a custom `apply_patch` declaration.
//! `ApplyPatchResponsesState` owns one bridge for a non-native executor stream.
//! That state lives in the executor helper, not in this module.

use std::collections::{HashMap, HashSet};

use gjson::{Kind, Value};

use crate::internal::client::codex::apply_patch::{
    description, go_json_string, is_custom_tool, parameters, wrap_input,
};

use super::apply_patch_events::{
    apply_patch_failure, apply_patch_input_delta, apply_patch_input_done, ApplyPatchCallState,
    ApplyPatchErrorState,
};
use super::apply_patch_input::ApplyPatchInputDecoder;
use super::join_raw_array;

struct Descriptor {
    name: String,
    source_priority: i32,
    direct: bool,
    order: usize,
    offset: usize,
    custom: bool,
}

struct Winner {
    offset: usize,
    custom: bool,
    source_priority: i32,
    direct: bool,
    order: usize,
}

struct ToolIndex {
    winners: HashMap<String, Winner>,
    affected: HashSet<String>,
}

/// Opts a non-Codex executor into the patch contract.
///
/// `original`, when supplied, is the Chat-completions request whose function
/// declarations win over a translated custom `apply_patch` tool of the same name.
pub fn normalize_apply_patch_responses_request(
    body: &[u8],
    original: Option<&[u8]>,
) -> Result<Vec<u8>, &'static str> {
    let prepared = match original {
        Some(original) => prefer_chat_function_patch_tools(original, body),
        None => body.to_vec(),
    };
    normalize_prepared(&prepared)
}

fn normalize_prepared(raw: &[u8]) -> Result<Vec<u8>, &'static str> {
    if serde_json::from_slice::<serde_json::Value>(raw).is_err() {
        return Err("invalid Responses request JSON");
    }
    let document = std::str::from_utf8(raw).map_err(|_| "invalid Responses request JSON")?;
    let root = gjson::parse(document);
    let index = ToolIndex::collect(document, &root);
    let mut output = raw.to_vec();
    if root.get("tools").kind() == Kind::Array {
        let normalized = normalize_tools(document, &root.get("tools"), "", &index);
        output = set_raw_path(&output, "tools", &normalized);
    }
    let mut patch_history = HashSet::new();
    for item in root.get("input").array() {
        if item.get("type").str() == "custom_tool_call"
            && item.get("name").str().trim() == "apply_patch"
        {
            patch_history.insert(item.get("call_id").str().to_owned());
        }
    }
    for (position, item) in root.get("input").array().iter().enumerate() {
        let path = format!("input.{position}");
        match item.get("type").str() {
            "additional_tools" => {
                let tools = item.get("tools");
                if tools.kind() == Kind::Array {
                    let normalized = normalize_tools(document, &tools, "", &index);
                    output = set_raw_path(&output, &format!("{path}.tools"), &normalized);
                }
            }
            "custom_tool_call" => {
                if item.get("name").str().trim() != "apply_patch" {
                    continue;
                }
                if item.get("input").kind() != Kind::String {
                    return Err("apply_patch history input must be a string");
                }
                let wrapped = wrap_input(item.get("input").str());
                output = set_json_string(&output, &format!("{path}.type"), "function_call");
                output = set_json_string(&output, &format!("{path}.arguments"), &wrapped);
                output = delete_path(&output, &format!("{path}.input"));
            }
            "custom_tool_call_output" => {
                if patch_history.contains(item.get("call_id").str()) {
                    output =
                        set_json_string(&output, &format!("{path}.type"), "function_call_output");
                }
            }
            _ => {}
        }
    }
    let choice = root.get("tool_choice");
    if choice.kind() == Kind::Object {
        let normalized = normalize_choice(choice.json(), &index);
        output = set_raw_path(&output, "tool_choice", normalized.as_bytes());
    }
    Ok(output)
}

pub(crate) fn prefer_chat_function_patch_tools(original: &[u8], declarations: &[u8]) -> Vec<u8> {
    let Ok(original_document) = std::str::from_utf8(original) else {
        return declarations.to_vec();
    };
    let mut ordinary = HashSet::new();
    for tool in gjson::parse(original_document).get("tools").array() {
        if tool.get("type").str() == "function" {
            ordinary.insert(tool.get("function.name").str().to_owned());
        }
    }
    if ordinary.is_empty() {
        return declarations.to_vec();
    }
    let Ok(document) = std::str::from_utf8(declarations) else {
        return declarations.to_vec();
    };
    let parsed = gjson::parse(document);
    let mut available = HashSet::new();
    for tool in parsed.get("tools").array() {
        if tool.get("type").str() == "function" {
            available.insert(tool.get("name").str().to_owned());
        }
    }
    let mut tools = Vec::new();
    for tool in parsed.get("tools").array() {
        let name = tool.get("name").str();
        if is_custom_tool(&tool) && ordinary.contains(name) && available.contains(name) {
            continue;
        }
        tools.push(tool.json().as_bytes().to_vec());
    }
    set_raw_path(declarations, "tools", &join_raw_array(&tools))
}

impl ToolIndex {
    fn collect(document: &str, root: &Value<'_>) -> Self {
        let descriptors = collect_descriptors(document, root);
        let mut winners = HashMap::new();
        let mut affected = HashSet::new();
        for descriptor in &descriptors {
            if descriptor.custom {
                affected.insert(descriptor.name.clone());
            }
            let replace = match winners.get(&descriptor.name) {
                None => true,
                Some(current) => descriptor_precedes(descriptor, current),
            };
            if replace {
                winners.insert(
                    descriptor.name.clone(),
                    Winner {
                        offset: descriptor.offset,
                        custom: descriptor.custom,
                        source_priority: descriptor.source_priority,
                        direct: descriptor.direct,
                        order: descriptor.order,
                    },
                );
            }
        }
        Self { winners, affected }
    }
}

fn descriptor_precedes(left: &Descriptor, current: &Winner) -> bool {
    if left.source_priority != current.source_priority {
        return left.source_priority < current.source_priority;
    }
    if left.direct != current.direct {
        return left.direct;
    }
    left.order < current.order
}

fn collect_descriptors(document: &str, root: &Value<'_>) -> Vec<Descriptor> {
    let mut descriptors = Vec::new();
    collect_source(document, &root.get("tools"), 0, &mut descriptors);
    for item in root.get("input").array() {
        if item.get("type").str() == "additional_tools" {
            collect_source(document, &item.get("tools"), 1, &mut descriptors);
        }
    }
    descriptors
}

fn collect_source(
    document: &str,
    tools: &Value<'_>,
    source_priority: i32,
    descriptors: &mut Vec<Descriptor>,
) {
    if tools.kind() != Kind::Array {
        return;
    }
    for tool in tools.array() {
        match tool.get("type").str().trim() {
            "" | "function" => {
                let name = tool_name(&tool);
                push_descriptor(
                    descriptors,
                    document,
                    &tool,
                    name.clone(),
                    source_priority,
                    true,
                );
            }
            "custom" => {
                let name = tool_name(&tool);
                push_descriptor(descriptors, document, &tool, name, source_priority, true);
            }
            "namespace" => {
                collect_namespace_children(document, &tool, source_priority, descriptors)
            }
            _ => {}
        }
    }
}

fn collect_namespace_children(
    document: &str,
    namespace_tool: &Value<'_>,
    source_priority: i32,
    descriptors: &mut Vec<Descriptor>,
) {
    let namespace_name = namespace_tool.get("name").str().trim();
    let mut children = namespace_tool.get("tools");
    if children.kind() != Kind::Array {
        children = namespace_tool.get("children");
    }
    if children.kind() != Kind::Array {
        return;
    }
    for child in children.array() {
        let child_name = tool_name(&child);
        if child_name.is_empty() {
            continue;
        }
        let qualified = qualify_namespace_tool_name(namespace_name, &child_name);
        match child.get("type").str().trim() {
            "" | "function" | "custom" => push_descriptor(
                descriptors,
                document,
                &child,
                qualified,
                source_priority,
                false,
            ),
            _ => {}
        }
    }
}

fn push_descriptor(
    descriptors: &mut Vec<Descriptor>,
    document: &str,
    tool: &Value<'_>,
    name: String,
    source_priority: i32,
    direct: bool,
) {
    if name.is_empty() {
        return;
    }
    let Some(offset) = value_offset(document, tool) else {
        return;
    };
    descriptors.push(Descriptor {
        name,
        source_priority,
        direct,
        order: descriptors.len(),
        offset,
        custom: is_custom_tool(tool),
    });
}

fn tool_name(tool: &Value<'_>) -> String {
    let name = tool.get("name").str().trim();
    if !name.is_empty() {
        return name.to_owned();
    }
    tool.get("function.name").str().trim().to_owned()
}

pub(crate) fn qualify_namespace_tool_name(namespace_name: &str, child_name: &str) -> String {
    let child_name = child_name.trim();
    let namespace_name = namespace_name.trim();
    if child_name.is_empty() || namespace_name.is_empty() || child_name.starts_with("mcp__") {
        return child_name.to_owned();
    }
    if child_name == namespace_name || child_name.starts_with(&format!("{namespace_name}__")) {
        return child_name.to_owned();
    }
    if namespace_name.ends_with("__") {
        format!("{namespace_name}{child_name}")
    } else {
        format!("{namespace_name}__{child_name}")
    }
}

fn normalize_tools(
    document: &str,
    tools: &Value<'_>,
    namespace: &str,
    index: &ToolIndex,
) -> Vec<u8> {
    let mut items = Vec::new();
    for tool in tools.array() {
        let mut item = tool.json().as_bytes().to_vec();
        if tool.get("type").str() == "namespace" {
            let child_namespace = tool.get("name").str();
            for key in ["tools", "children"] {
                let children = tool.get(key);
                if children.kind() == Kind::Array {
                    let normalized = normalize_tools(document, &children, child_namespace, index);
                    item = upsert_object_key(&item, key, &normalized);
                    break;
                }
            }
        } else {
            let mut name = tool.get("name").str().to_owned();
            if name.is_empty() {
                name = tool.get("function.name").str().to_owned();
            }
            let qualified = qualify_namespace_tool_name(namespace, &name);
            if let Some(winner) = index.winners.get(&qualified) {
                if index.affected.contains(&qualified) {
                    if value_offset(document, &tool) != Some(winner.offset) {
                        continue;
                    }
                    if is_custom_tool(&tool) {
                        item = rewrite_custom_tool(&item, &tool);
                    }
                }
            }
        }
        items.push(item);
    }
    join_raw_array(&items)
}

fn rewrite_custom_tool(item: &[u8], tool: &Value<'_>) -> Vec<u8> {
    let mut item = set_json_string(item, "type", "function");
    item = set_json_string(&item, "description", &description(tool));
    item = set_raw_path(&item, "parameters", &parameters());
    delete_path(&item, "format")
}

fn normalize_choice(choice_json: &str, index: &ToolIndex) -> Vec<u8> {
    let parsed = gjson::parse(choice_json);
    let mut output = choice_json.as_bytes().to_vec();
    let qualified =
        qualify_namespace_tool_name(parsed.get("namespace").str(), parsed.get("name").str());
    if index
        .winners
        .get(&qualified)
        .is_some_and(|winner| winner.custom)
        && parsed.get("type").str() == "custom"
    {
        output = set_json_string(&output, "type", "function");
    }
    for (position, child) in parsed.get("tools").array().iter().enumerate() {
        let normalized = normalize_choice(child.json(), index);
        output = set_raw_path(&output, &format!("tools.{position}"), &normalized);
    }
    output
}

fn value_offset(document: &str, value: &Value<'_>) -> Option<usize> {
    let raw = value.json();
    if raw.is_empty() {
        return None;
    }
    let start = raw.as_ptr() as usize;
    let base = document.as_ptr() as usize;
    if start < base {
        return None;
    }
    let offset = start - base;
    if offset + raw.len() > document.len() {
        return None;
    }
    (document.as_bytes()[offset..offset + raw.len()] == *raw.as_bytes()).then_some(offset)
}

pub(crate) fn set_json_string(data: &[u8], path: &str, value: &str) -> Vec<u8> {
    let literal = go_json_string(value);
    set_raw_path(data, path, literal.as_bytes())
}

pub(crate) fn set_json_i64(data: &[u8], path: &str, value: i64) -> Vec<u8> {
    set_raw_path(data, path, value.to_string().as_bytes())
}

fn delete_root_key(data: &[u8], key: &str) -> Vec<u8> {
    let Ok(document) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    if gjson::parse(document).kind() != Kind::Object {
        return data.to_vec();
    }
    delete_key(document, key).into_bytes()
}

/// Removes the first named member while retaining raw sibling values and order.
pub(crate) fn delete_raw_path(data: &[u8], path: &str) -> Vec<u8> {
    let Ok(document) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let (parent, key) = split_last(path);
    let object = if parent.is_empty() {
        gjson::parse(document)
    } else {
        gjson::get(document, parent)
    };
    let Some(members) = object_members(object.json()) else {
        return data.to_vec();
    };
    let mut removed = false;
    let parts: Vec<String> = members
        .into_iter()
        .filter_map(|(name, raw)| {
            if name == key && !removed {
                removed = true;
                None
            } else {
                Some(raw)
            }
        })
        .collect();
    if !removed {
        return data.to_vec();
    }
    let updated = format!("{{{}}}", parts.join(","));
    splice(data, document, object.json(), updated.as_bytes()).unwrap_or_else(|| data.to_vec())
}

pub(crate) fn set_raw_path(data: &[u8], path: &str, replacement: &[u8]) -> Vec<u8> {
    let Ok(document) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let Ok(replacement) = std::str::from_utf8(replacement) else {
        return data.to_vec();
    };
    let (parent, key) = split_last(path);
    if parent.is_empty() {
        let existing = gjson::get(document, path);
        if existing.exists() {
            if let Some(output) = splice(data, document, existing.json(), replacement.as_bytes()) {
                return output;
            }
        }
        return upsert_root_key(data, key, replacement);
    }
    let parent_value = gjson::get(document, parent);
    if parent_value.kind() == Kind::Array {
        let existing = gjson::get(document, path);
        if existing.exists() {
            if let Some(output) = splice(data, document, existing.json(), replacement.as_bytes()) {
                return output;
            }
        }
        return data.to_vec();
    }
    if !parent_value.exists() || parent_value.kind() != Kind::Object {
        return data.to_vec();
    }
    let updated = upsert_key(parent_value.json(), key, replacement);

    splice(data, document, parent_value.json(), updated.as_bytes()).unwrap_or_else(|| data.to_vec())
}

fn delete_path(data: &[u8], path: &str) -> Vec<u8> {
    let Ok(document) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let (parent, key) = split_last(path);
    if parent.is_empty() {
        return data.to_vec();
    }
    let parent_value = gjson::get(document, parent);
    if !parent_value.exists() || parent_value.kind() != Kind::Object {
        return data.to_vec();
    }
    let updated = delete_key(parent_value.json(), key);
    splice(data, document, parent_value.json(), updated.as_bytes()).unwrap_or_else(|| data.to_vec())
}

fn upsert_root_key(data: &[u8], key: &str, raw_value: &str) -> Vec<u8> {
    let Ok(document) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let root = gjson::parse(document);
    if root.kind() != Kind::Object {
        return data.to_vec();
    }
    let updated = upsert_key(root.json(), key, raw_value);
    updated.into_bytes()
}

fn upsert_object_key(object: &[u8], key: &str, raw_value: &[u8]) -> Vec<u8> {
    let Ok(object) = std::str::from_utf8(object) else {
        return object.to_vec();
    };
    let Ok(raw_value) = std::str::from_utf8(raw_value) else {
        return object.to_vec();
    };
    upsert_key(object, key, raw_value).into_bytes()
}

fn upsert_key(object: &str, key: &str, raw_value: &str) -> String {
    let Some(members) = object_members(object) else {
        return object.to_owned();
    };
    let mut found = false;
    let mut parts = Vec::with_capacity(members.len() + 1);
    let encoded_key = serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_owned());
    for (name, raw_member) in members {
        if name == key && !found {
            found = true;
            parts.push(format!("{encoded_key}:{raw_value}"));
        } else {
            parts.push(raw_member);
        }
    }
    if !found {
        parts.push(format!("{encoded_key}:{raw_value}"));
    }
    format!("{{{}}}", parts.join(","))
}

fn delete_key(object: &str, key: &str) -> String {
    let Some(members) = object_members(object) else {
        return object.to_owned();
    };
    let parts = members
        .into_iter()
        .filter(|(name, _)| name != key)
        .map(|(_, raw)| raw)
        .collect::<Vec<_>>();
    format!("{{{}}}", parts.join(","))
}

fn object_members(object: &str) -> Option<Vec<(String, String)>> {
    let bytes = object.as_bytes();
    let mut index = skip_ascii_ws(bytes, 0);
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    index += 1;
    let mut members = Vec::new();
    loop {
        index = skip_ascii_ws(bytes, index);
        if bytes.get(index) == Some(&b'}') {
            index += 1;
            break;
        }
        if !members.is_empty() {
            if bytes.get(index) != Some(&b',') {
                return None;
            }
            index = skip_ascii_ws(bytes, index + 1);
        }
        let start = index;
        let (name, next) = decode_string(bytes, index).ok()?;
        index = skip_ascii_ws(bytes, next);
        if bytes.get(index) != Some(&b':') {
            return None;
        }
        index = skip_ascii_ws(bytes, index + 1);
        index = skip_json_value(bytes, index)?;
        let raw = std::str::from_utf8(&bytes[start..index]).ok()?.to_owned();
        members.push((name, raw));
    }
    index = skip_ascii_ws(bytes, index);
    (index == bytes.len()).then_some(members)
}

fn split_last(path: &str) -> (&str, &str) {
    match path.rsplit_once('.') {
        Some((parent, key)) => (parent, key),
        None => ("", path),
    }
}

fn splice(data: &[u8], document: &str, raw: &str, replacement: &[u8]) -> Option<Vec<u8>> {
    if raw.is_empty() {
        return None;
    }
    let start = raw.as_ptr() as usize;
    let base = document.as_ptr() as usize;
    if start < base {
        return None;
    }
    let offset = start - base;
    if offset + raw.len() > data.len() {
        return None;
    }
    let mut output = Vec::with_capacity(data.len() - raw.len() + replacement.len());
    output.extend_from_slice(&data[..offset]);
    output.extend_from_slice(replacement);
    output.extend_from_slice(&data[offset + raw.len()..]);
    Some(output)
}

fn skip_ascii_ws(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    index
}

fn decode_string(bytes: &[u8], mut index: usize) -> Result<(String, usize), ()> {
    if bytes.get(index) != Some(&b'"') {
        return Err(());
    }
    index += 1;
    let mut decoded = String::new();
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        match byte {
            b'"' => return Ok((decoded, index)),
            b'\\' => {
                let escape = *bytes.get(index).ok_or(())?;
                index += 1;
                match escape {
                    b'"' => decoded.push('"'),
                    b'\\' => decoded.push('\\'),
                    b'/' => decoded.push('/'),
                    b'b' => decoded.push('\u{0008}'),
                    b'f' => decoded.push('\u{000c}'),
                    b'n' => decoded.push('\n'),
                    b'r' => decoded.push('\r'),
                    b't' => decoded.push('\t'),
                    b'u' => {
                        let hex = bytes.get(index..index + 4).ok_or(())?;
                        index += 4;
                        let code =
                            u32::from_str_radix(std::str::from_utf8(hex).map_err(|_| ())?, 16)
                                .map_err(|_| ())?;
                        decoded.push(char::from_u32(code).ok_or(())?);
                    }
                    _ => return Err(()),
                }
            }
            byte if byte < 0x20 => return Err(()),
            byte => {
                let width = match byte {
                    0x00..=0x7f => 1,
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf7 => 4,
                    _ => return Err(()),
                };
                let start = index - 1;
                let text = std::str::from_utf8(bytes.get(start..start + width).ok_or(())?)
                    .map_err(|_| ())?;
                decoded.push_str(text);
                index = start + width;
            }
        }
    }
    Err(())
}

fn skip_json_value(bytes: &[u8], index: usize) -> Option<usize> {
    let byte = *bytes.get(index)?;
    match byte {
        b'"' => decode_string(bytes, index).ok().map(|(_, next)| next),
        b'{' | b'[' => skip_container(bytes, index),
        b't' => consume_literal(bytes, index, b"true"),
        b'f' => consume_literal(bytes, index, b"false"),
        b'n' => consume_literal(bytes, index, b"null"),
        b'-' | b'0'..=b'9' => skip_number(bytes, index),
        _ => None,
    }
}

fn consume_literal(bytes: &[u8], index: usize, literal: &[u8]) -> Option<usize> {
    bytes
        .get(index..index + literal.len())
        .filter(|slice| *slice == literal)?;
    Some(index + literal.len())
}

fn skip_number(bytes: &[u8], mut index: usize) -> Option<usize> {
    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    let start = index;
    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
        index += 1;
    }
    if index == start {
        return None;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == fraction {
            return None;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == exponent {
            return None;
        }
    }
    Some(index)
}

fn skip_container(bytes: &[u8], mut index: usize) -> Option<usize> {
    let open = *bytes.get(index)?;
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                continue;
            }
            if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return (byte == close).then_some(index);
                }
            }
            _ => {}
        }
    }
    None
}

#[derive(Clone)]
struct BridgeTool {
    name: String,
    local_name: String,
    namespace: String,
    custom: bool,
    source_priority: i32,
    direct: bool,
    order: usize,
}

fn bridge_tool_precedes(left: &BridgeTool, current: &BridgeTool) -> bool {
    if left.source_priority != current.source_priority {
        return left.source_priority < current.source_priority;
    }
    if left.direct != current.direct {
        return left.direct;
    }
    left.order < current.order
}

fn collect_bridge_tools(raw: &[u8]) -> HashMap<String, BridgeTool> {
    let Ok(document) = std::str::from_utf8(raw) else {
        return HashMap::new();
    };
    let root = gjson::parse(document);
    let mut descriptors = Vec::new();
    collect_bridge_source(&root.get("tools"), 0, &mut descriptors);
    for item in root.get("input").array() {
        if item.get("type").str() == "additional_tools" {
            collect_bridge_source(&item.get("tools"), 1, &mut descriptors);
        }
    }
    let mut winners = HashMap::new();
    for descriptor in descriptors {
        let replace = match winners.get(&descriptor.name) {
            None => true,
            Some(current) => bridge_tool_precedes(&descriptor, current),
        };
        if replace {
            winners.insert(descriptor.name.clone(), descriptor);
        }
    }
    winners
}

fn collect_bridge_source(
    tools: &Value<'_>,
    source_priority: i32,
    descriptors: &mut Vec<BridgeTool>,
) {
    if tools.kind() != Kind::Array {
        return;
    }
    for tool in tools.array() {
        match tool.get("type").str().trim() {
            "" | "function" | "custom" => {
                let name = tool_name(&tool);
                push_bridge_tool(
                    descriptors,
                    &tool,
                    name.clone(),
                    name,
                    String::new(),
                    source_priority,
                    true,
                );
            }
            "namespace" => collect_bridge_children(&tool, source_priority, descriptors),
            _ => {}
        }
    }
}

fn collect_bridge_children(
    namespace_tool: &Value<'_>,
    source_priority: i32,
    descriptors: &mut Vec<BridgeTool>,
) {
    let namespace_name = namespace_tool.get("name").str().trim().to_owned();
    let mut children = namespace_tool.get("tools");
    if children.kind() != Kind::Array {
        children = namespace_tool.get("children");
    }
    if children.kind() != Kind::Array {
        return;
    }
    for child in children.array() {
        let child_name = tool_name(&child);
        if child_name.is_empty() {
            continue;
        }
        let qualified = qualify_namespace_tool_name(&namespace_name, &child_name);
        match child.get("type").str().trim() {
            "" | "function" | "custom" => push_bridge_tool(
                descriptors,
                &child,
                qualified,
                child_name,
                namespace_name.clone(),
                source_priority,
                false,
            ),
            _ => {}
        }
    }
}

fn push_bridge_tool(
    descriptors: &mut Vec<BridgeTool>,
    tool: &Value<'_>,
    name: String,
    local_name: String,
    namespace: String,
    source_priority: i32,
    direct: bool,
) {
    if name.is_empty() {
        return;
    }
    descriptors.push(BridgeTool {
        name,
        local_name,
        namespace,
        custom: is_custom_tool(tool),
        source_priority,
        direct,
        order: descriptors.len(),
    });
}

struct ArgumentSnapshot {
    exists: bool,
    is_string: bool,
    text: String,
}

impl ArgumentSnapshot {
    fn capture(value: &Value<'_>) -> Self {
        Self {
            exists: value.exists(),
            is_string: value.kind() == Kind::String,
            text: value.str().to_owned(),
        }
    }
}

fn assign_json_value(object: &[u8], key: &str, value: &Value<'_>) -> Vec<u8> {
    if value.kind() == Kind::String {
        set_json_string(object, key, value.str())
    } else {
        set_raw_path(object, key, value.json().as_bytes())
    }
}

struct ResponsesPatchRecord {
    state: ApplyPatchCallState,
    kind: String,
    qualified: String,
    source: String,
    patch: bool,
    named: bool,
    added: bool,
    input_done: bool,
    item_done: bool,
    snapshot: String,
    completed_item: Vec<u8>,
    has_snapshot: bool,
    pending: Vec<Vec<u8>>,
    evidence: Option<String>,
}

impl ResponsesPatchRecord {
    fn new() -> Self {
        Self {
            state: ApplyPatchCallState {
                output_index: -1,
                ..ApplyPatchCallState::default()
            },
            kind: String::new(),
            qualified: String::new(),
            source: String::new(),
            patch: false,
            named: false,
            added: false,
            input_done: false,
            item_done: false,
            snapshot: String::new(),
            completed_item: Vec::new(),
            has_snapshot: false,
            pending: Vec::new(),
            evidence: None,
        }
    }
}

fn identity_ready(record: &ResponsesPatchRecord) -> bool {
    !record.state.item_id.is_empty()
        && !record.state.call_id.is_empty()
        && record.state.output_index >= 0
}

/// Events produced for one payload. A terminal failure carries both the
/// `response.failed` payload and the original conversion error.
pub(crate) struct ApplyPatchTransform {
    pub(crate) events: Vec<Vec<u8>>,
    pub(crate) error: Option<String>,
}

/// Converts JSON event payloads for one response, without SSE framing.
///
/// A known custom name is provenance, not readiness. Identity evidence is
/// retained before the call receives its name. One bridge belongs to one
/// response.
pub(crate) struct ApplyPatchResponsesBridge {
    errors: ApplyPatchErrorState,
    tools: HashMap<String, BridgeTool>,
    records: Vec<ResponsesPatchRecord>,
    by_item_id: HashMap<String, usize>,
    by_call_id: HashMap<String, usize>,
    by_output_index: HashMap<i64, usize>,
    sequence: i64,
    last_sequence: i64,
    response_id: String,
    failed: bool,
    terminal: bool,
    active: bool,
    converted: bool,
}

impl ApplyPatchResponsesBridge {
    /// Resolves the original declarations before any normalization.
    pub(crate) fn new(original_request: &[u8]) -> Self {
        let tools = collect_bridge_tools(original_request);
        let active = tools.values().any(|tool| tool.custom);
        Self {
            errors: ApplyPatchErrorState::default(),
            tools,
            records: Vec::new(),
            by_item_id: HashMap::new(),
            by_call_id: HashMap::new(),
            by_output_index: HashMap::new(),
            sequence: 0,
            last_sequence: 0,
            response_id: String::new(),
            failed: false,
            terminal: false,
            active,
            converted: false,
        }
    }

    pub(crate) fn tool_input_error(&self) -> Option<&str> {
        self.errors.tool_input_error()
    }

    /// Terminates an executor-owned bridge with the one-shot failure contract.
    pub(crate) fn fail(&mut self, error: &str) -> ApplyPatchTransform {
        self.failure(error)
    }

    fn next_sequence(&mut self) -> i64 {
        self.sequence += 1;
        self.sequence
    }

    fn failure(&mut self, error: &str) -> ApplyPatchTransform {
        if self.failed || self.terminal {
            return ApplyPatchTransform {
                events: Vec::new(),
                error: None,
            };
        }
        self.failed = true;
        self.errors.set_tool_input_error(Some(error.to_owned()));
        let payload = apply_patch_failure(&self.response_id, self.next_sequence());
        ApplyPatchTransform {
            events: vec![payload],
            error: Some(error.to_owned()),
        }
    }

    fn descriptor(&self, namespace: &str, name: &str) -> Option<BridgeTool> {
        let qualified = qualify_namespace_tool_name(namespace, name);
        self.tools.get(&qualified).cloned()
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// Reports whether a namespace still owns a winning custom patch child.
    pub(crate) fn namespace_has_custom(&self, namespace: &str) -> bool {
        self.tools
            .values()
            .any(|tool| tool.namespace == namespace && tool.custom)
    }

    /// Qualified name and custom flag. A missing declaration is not custom.
    pub(crate) fn child_tool(&self, namespace: &str, name: &str) -> (String, bool) {
        match self.descriptor(namespace, name) {
            Some(tool) => (tool.name, tool.custom),
            None => (String::new(), false),
        }
    }

    /// Checks every supplied identity. Conflicting unmatched keys and multiple
    /// matched records keep their evidence until patch provenance is known.
    fn resolve(&mut self, event_raw: &[u8], item_raw: &[u8]) -> Result<usize, String> {
        let event_doc = std::str::from_utf8(event_raw)
            .map_err(|_| "invalid Responses event JSON".to_owned())?;
        let item_doc =
            std::str::from_utf8(item_raw).map_err(|_| "invalid Responses event JSON".to_owned())?;
        let event = gjson::parse(event_doc);
        let item = gjson::parse(item_doc);
        let id_event = event.get("item_id").str().to_owned();
        let id_item = item.get("id").str().to_owned();
        let call_event = event.get("call_id").str().to_owned();
        let call_item = item.get("call_id").str().to_owned();
        let index_exists = event.get("output_index").exists();
        let index_value = event.get("output_index").i64();
        let item_type = item.get("type").str().to_owned();
        let item_name = item.get("name").str().to_owned();
        let item_namespace = item.get("namespace").str().to_owned();
        let known = self.descriptor(&item_namespace, &item_name);

        let mut matched = Vec::new();
        for id in [&id_event, &id_item] {
            if !id.is_empty() {
                if let Some(index) = self.by_item_id.get(id).copied() {
                    if !matched.contains(&index) {
                        matched.push(index);
                    }
                }
            }
        }
        for id in [&call_event, &call_item] {
            if !id.is_empty() {
                if let Some(index) = self.by_call_id.get(id).copied() {
                    if !matched.contains(&index) {
                        matched.push(index);
                    }
                }
            }
        }
        if index_exists {
            if let Some(index) = self.by_output_index.get(&index_value).copied() {
                if !matched.contains(&index) {
                    matched.push(index);
                }
            }
        }
        let record_index = match (0..self.records.len()).find(|index| matched.contains(index)) {
            Some(index) => index,
            None => {
                self.records.push(ResponsesPatchRecord::new());
                self.records.len() - 1
            }
        };
        let state_item = self.records[record_index].state.item_id.clone();
        let state_call = self.records[record_index].state.call_id.clone();
        let state_index = self.records[record_index].state.output_index;
        let mut bad = matched.len() > 1;
        for id in [&id_event, &id_item] {
            if !id.is_empty() && !state_item.is_empty() && state_item != *id {
                bad = true;
            }
        }
        for id in [&call_event, &call_item] {
            if !id.is_empty() && !state_call.is_empty() && state_call != *id {
                bad = true;
            }
        }
        if !id_event.is_empty() && !id_item.is_empty() && id_event != id_item {
            bad = true;
        }
        if !call_event.is_empty() && !call_item.is_empty() && call_event != call_item {
            bad = true;
        }
        if index_exists && state_index >= 0 && state_index != index_value {
            bad = true;
        }
        let mut incoming_patch =
            known.as_ref().is_some_and(|tool| tool.custom) && item_type != "custom_tool_call";
        if bad {
            let identity = "conflicting apply_patch call identity".to_owned();
            self.records[record_index].evidence = Some(identity.clone());
            for index in &matched {
                self.records[*index].evidence = Some(identity.clone());
                if self.records[*index].patch {
                    incoming_patch = true;
                }
            }
            for id in [&id_event, &id_item] {
                if !id.is_empty() && !self.by_item_id.contains_key(id) {
                    self.by_item_id.insert(id.clone(), record_index);
                }
            }
            for id in [&call_event, &call_item] {
                if !id.is_empty() && !self.by_call_id.contains_key(id) {
                    self.by_call_id.insert(id.clone(), record_index);
                }
            }
            if index_exists && !self.by_output_index.contains_key(&index_value) {
                self.by_output_index.insert(index_value, record_index);
            }
            if self.records[record_index].patch || incoming_patch {
                return Err(identity);
            }
        } else {
            for id in [&id_event, &id_item] {
                if !id.is_empty() {
                    self.records[record_index].state.item_id.clone_from(id);
                    self.by_item_id.insert(id.clone(), record_index);
                }
            }
            for id in [&call_event, &call_item] {
                if !id.is_empty() {
                    self.records[record_index].state.call_id.clone_from(id);
                    self.by_call_id.insert(id.clone(), record_index);
                }
            }
            if index_exists {
                self.records[record_index].state.output_index = index_value;
                self.by_output_index.insert(index_value, record_index);
            }
        }
        if !item_type.is_empty() {
            let current_kind = self.records[record_index].kind.clone();
            if !current_kind.is_empty() && current_kind != item_type {
                self.records[record_index].evidence =
                    Some("conflicting apply_patch call type".to_owned());
            }
            if current_kind.is_empty() {
                self.records[record_index].kind = item_type;
            }
        }
        if !item_name.is_empty() {
            let mut qualified = qualify_namespace_tool_name(&item_namespace, &item_name);
            let mut state_name = item_name.clone();
            let mut state_namespace = item_namespace.clone();
            if let Some(tool) = &known {
                qualified.clone_from(&tool.name);
                state_name.clone_from(&tool.local_name);
                state_namespace.clone_from(&tool.namespace);
            }
            if self.records[record_index].named && self.records[record_index].qualified != qualified
            {
                self.records[record_index].evidence =
                    Some("conflicting apply_patch call name".to_owned());
            }
            self.records[record_index].named = true;
            self.records[record_index].qualified = qualified;
            self.records[record_index].state.name = state_name;
            self.records[record_index].state.namespace = state_namespace;
        }
        if incoming_patch {
            self.records[record_index].patch = true;
        }
        if self.records[record_index].patch {
            if let Some(evidence) = self.records[record_index].evidence.clone() {
                return Err(evidence);
            }
        }
        Ok(record_index)
    }

    /// Retains every supplied key before a folded dispatcher reveals its child.
    /// A dispatcher name is not the selected child name.
    pub(crate) fn check_identity(&mut self, event: &[u8]) -> Result<(), String> {
        let document =
            std::str::from_utf8(event).map_err(|_| "invalid Responses event JSON".to_owned())?;
        let root = gjson::parse(document);
        let item_raw = if root.get("item").exists() {
            let raw = root.get("item").json().as_bytes().to_vec();
            delete_root_key(&delete_root_key(&raw, "name"), "namespace")
        } else {
            Vec::new()
        };
        self.resolve(event, &item_raw).map(|_| ())
    }

    fn restore_item(&self, mut item: Vec<u8>, index: usize, input: &str, added: bool) -> Vec<u8> {
        let record = &self.records[index];
        if record.patch {
            item = set_json_string(&item, "type", "custom_tool_call");
            item = delete_root_key(&item, "arguments");
            item = set_json_string(&item, "input", input);
        }
        if let Some(tool) = self.tools.get(&record.qualified) {
            if !tool.namespace.is_empty() {
                item = set_json_string(&item, "name", &tool.local_name);
                item = set_json_string(&item, "namespace", &tool.namespace);
            }
        }
        if self.records[index].patch && !added {
            if !self.records[index].state.item_id.is_empty() {
                item = set_json_string(&item, "id", &self.records[index].state.item_id);
            }
            if !self.records[index].state.call_id.is_empty() {
                item = set_json_string(&item, "call_id", &self.records[index].state.call_id);
            }
            item = set_json_string(&item, "name", &self.records[index].state.name);
        }
        item
    }

    fn item_event(&mut self, kind: &str, item: &[u8], index: usize) -> Vec<u8> {
        let output_index = self.records[index].state.output_index;
        let mut out = br#"{"type":"","output_index":0,"sequence_number":0,"item":{}}"#.to_vec();
        out = set_json_string(&out, "type", kind);
        out = set_json_i64(&out, "output_index", output_index);
        let sequence = self.next_sequence();
        out = set_json_i64(&out, "sequence_number", sequence);
        set_raw_path(&out, "item", item)
    }

    fn snapshot(
        &mut self,
        index: usize,
        arguments: &ArgumentSnapshot,
        final_snapshot: bool,
    ) -> Result<(), String> {
        if !arguments.exists {
            return Ok(());
        }
        if !arguments.is_string {
            return Err("apply_patch arguments snapshot must be a string".to_owned());
        }
        if arguments.text.is_empty() && !final_snapshot {
            return Ok(());
        }
        let mut decoder = ApplyPatchInputDecoder::default();
        decoder
            .finish(arguments.text.as_bytes())
            .map_err(|error| error.message().to_owned())?;
        let decoded = decoder.input().to_owned();
        if self.records[index].has_snapshot {
            let previous_raw = self.records[index].snapshot.clone();
            let mut previous = ApplyPatchInputDecoder::default();
            let _ = previous.finish(previous_raw.as_bytes());
            if previous.input() != decoded {
                return Err("conflicting apply_patch arguments snapshot".to_owned());
            }
        }
        let streamed = self.records[index].state.decoder.input().to_owned();
        if !decoded.starts_with(&streamed) {
            return Err("apply_patch snapshot conflicts with streamed input".to_owned());
        }
        self.records[index].snapshot.clone_from(&arguments.text);
        self.records[index].has_snapshot = true;
        Ok(())
    }

    fn patch_event(&mut self, raw: &[u8], index: usize) -> Result<Vec<Vec<u8>>, String> {
        if !identity_ready(&self.records[index]) {
            return Err("unresolved apply_patch call identity".to_owned());
        }
        let document =
            std::str::from_utf8(raw).map_err(|_| "invalid Responses event JSON".to_owned())?;
        let root = gjson::parse(document);
        let kind = root.get("type").str().to_owned();
        let item_exists = root.get("item").exists();
        let item_type = root.get("item.type").str().to_owned();
        let item_raw = root.get("item").json().as_bytes().to_vec();
        let root_arguments = ArgumentSnapshot::capture(&root.get("arguments"));
        let item_arguments = ArgumentSnapshot::capture(&root.get("item.arguments"));
        let fragment = root.get("delta").str().to_owned();
        self.converted = true;
        let mut out = Vec::new();
        if item_exists {
            if item_type != "function_call" {
                return Err("conflicting apply_patch call type".to_owned());
            }
            self.snapshot(index, &item_arguments, kind == "response.output_item.done")?;
        }
        if !self.records[index].added {
            let mut added = if item_exists {
                item_raw.clone()
            } else {
                br#"{"type":"function_call","name":"","arguments":""}"#.to_vec()
            };
            let name = self.records[index].state.name.clone();
            let item_id = self.records[index].state.item_id.clone();
            let call_id = self.records[index].state.call_id.clone();
            let namespace = self.records[index].state.namespace.clone();
            added = set_json_string(&added, "name", &name);
            if !item_id.is_empty() {
                added = set_json_string(&added, "id", &item_id);
            }
            if !call_id.is_empty() {
                added = set_json_string(&added, "call_id", &call_id);
            }
            if !namespace.is_empty() {
                added = set_json_string(&added, "namespace", &namespace);
            }
            let restored = self.restore_item(added, index, "", true);
            out.push(self.item_event("response.output_item.added", &restored, index));
            self.records[index].added = true;
        }
        match kind.as_str() {
            "response.function_call_arguments.delta" => {
                if self.records[index].input_done {
                    if !fragment.is_empty() {
                        return Err("apply_patch arguments received after completion".to_owned());
                    }
                    return Ok(out);
                }
                self.records[index].source.push_str(&fragment);
                let preview = self.records[index]
                    .state
                    .push_arguments(fragment.as_bytes())
                    .map_err(|error| error.message().to_owned())?;
                if self.records[index].has_snapshot {
                    let snapshot_raw = self.records[index].snapshot.clone();
                    let mut snapshot = ApplyPatchInputDecoder::default();
                    let _ = snapshot.finish(snapshot_raw.as_bytes());
                    let snapshot_input = snapshot.input().to_owned();
                    if !snapshot_input.starts_with(self.records[index].state.decoder.input()) {
                        return Err("apply_patch stream conflicts with snapshot".to_owned());
                    }
                }
                if !preview.is_empty() {
                    let sequence = self.next_sequence();
                    out.push(apply_patch_input_delta(
                        &self.records[index].state,
                        &preview,
                        sequence,
                    ));
                }
            }
            "response.function_call_arguments.done" | "response.output_item.done" => {
                let arguments = if item_exists {
                    &item_arguments
                } else {
                    &root_arguments
                };
                if arguments.exists {
                    self.snapshot(index, arguments, true)?;
                }
                let final_text = if self.records[index].has_snapshot {
                    self.records[index].snapshot.clone()
                } else {
                    self.records[index].source.clone()
                };
                let (tail, input) = self.records[index]
                    .state
                    .finish_arguments(final_text.as_bytes())
                    .map_err(|error| error.message().to_owned())?;
                if !self.records[index].input_done {
                    if !tail.is_empty() && !self.records[index].source.is_empty() {
                        let sequence = self.next_sequence();
                        out.push(apply_patch_input_delta(
                            &self.records[index].state,
                            &tail,
                            sequence,
                        ));
                    }
                    let sequence = self.next_sequence();
                    out.push(apply_patch_input_done(
                        &self.records[index].state,
                        &input,
                        sequence,
                    ));
                    self.records[index].input_done = true;
                }
                if kind == "response.output_item.done" && !self.records[index].item_done {
                    let restored = self.restore_item(item_raw, index, &input, false);
                    self.records[index].completed_item = restored.clone();
                    out.push(self.item_event(&kind, &restored, index));
                    self.records[index].item_done = true;
                }
            }
            _ => {}
        }
        Ok(out)
    }

    fn transform_item_event(&mut self, raw: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        let document =
            std::str::from_utf8(raw).map_err(|_| "invalid Responses event JSON".to_owned())?;
        let root = gjson::parse(document);
        let original_item_exists = root.get("item").exists();
        let root_type = root.get("type").str().to_owned();
        let root_arguments = ArgumentSnapshot::capture(&root.get("arguments"));
        let item_arguments = ArgumentSnapshot::capture(&root.get("item.arguments"));
        let item_raw = if original_item_exists {
            root.get("item").json().as_bytes().to_vec()
        } else if root.get("name").exists() {
            let mut identity = br#"{"type":"function_call"}"#.to_vec();
            for key in ["name", "namespace", "call_id"] {
                let value = root.get(key);
                if value.exists() {
                    identity = assign_json_value(&identity, key, &value);
                }
            }
            identity
        } else {
            Vec::new()
        };
        let index = self.resolve(raw, &item_raw)?;
        let named = self.records[index].named;
        let kind = self.records[index].kind.clone();
        let patch = self.records[index].patch;
        let ready = identity_ready(&self.records[index]);
        // A known name is not readiness. Keep the source events until both
        // upstream IDs and the output index can identify every emitted event.
        if (!named && (kind.is_empty() || kind == "function_call")) || (patch && !ready) {
            if patch {
                let arguments = if original_item_exists {
                    &item_arguments
                } else {
                    &root_arguments
                };
                let final_snapshot = root_type == "response.output_item.done"
                    || root_type == "response.function_call_arguments.done";
                self.snapshot(index, arguments, final_snapshot)?;
            }
            self.records[index].pending.push(raw.to_vec());
            return Ok(Vec::new());
        }
        if patch {
            let mut pending = std::mem::take(&mut self.records[index].pending);
            pending.push(raw.to_vec());
            let mut out = Vec::new();
            for event in pending {
                out.extend(self.patch_event(&event, index)?);
            }
            return Ok(out);
        }
        let mut out = std::mem::take(&mut self.records[index].pending);
        let qualified = self.records[index].qualified.clone();
        let mut emitted = raw.to_vec();
        if original_item_exists
            && kind == "function_call"
            && self
                .tools
                .get(&qualified)
                .is_some_and(|tool| !tool.namespace.is_empty())
        {
            let restored = self.restore_item(item_raw.clone(), index, "", false);
            emitted = set_raw_path(&emitted, "item", &restored);
        }
        if root_type == "response.output_item.done" && !item_raw.is_empty() {
            let completed = gjson::get(std::str::from_utf8(&emitted).unwrap_or(""), "item")
                .json()
                .as_bytes()
                .to_vec();
            self.records[index].item_done = true;
            self.records[index].completed_item = completed;
        }
        out.push(emitted);
        Ok(out)
    }

    fn envelope(&mut self, raw: &[u8], stream: bool) -> Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let document =
            std::str::from_utf8(raw).map_err(|_| "invalid Responses event JSON".to_owned())?;
        let root = gjson::parse(document);
        let (path, response_json) = if root.get("response").exists() {
            ("response.output", root.get("response").json().to_owned())
        } else {
            ("output", document.to_owned())
        };
        let slots: Vec<Vec<u8>> = gjson::parse(&response_json)
            .get("output")
            .array()
            .iter()
            .map(|item| item.json().as_bytes().to_vec())
            .collect();
        let original_len = slots.len();
        let mut current = raw.to_vec();
        let mut preceding = Vec::new();
        let mut seen = HashSet::new();
        let mut items = Vec::new();
        for (position, slot) in slots.iter().enumerate() {
            let slot_text = std::str::from_utf8(slot).unwrap_or("");
            let id = gjson::get(slot_text, "id").str().to_owned();
            let call_id = gjson::get(slot_text, "call_id").str().to_owned();
            let type_name = gjson::get(slot_text, "type").str().to_owned();
            let mut index = position as i64;
            let known = if !id.is_empty() {
                self.by_item_id.get(&id).copied()
            } else {
                None
            }
            .or_else(|| {
                if call_id.is_empty() {
                    None
                } else {
                    self.by_call_id.get(&call_id).copied()
                }
            });
            if let Some(known) = known {
                if self.records[known].state.output_index >= 0 {
                    index = self.records[known].state.output_index;
                }
            } else if !id.is_empty() || !call_id.is_empty() {
                if let Some(previous) = self.by_output_index.get(&(position as i64)).copied() {
                    let previous = &self.records[previous];
                    if !previous.state.item_id.is_empty() || !previous.state.call_id.is_empty() {
                        for record in &self.records {
                            if record.state.output_index >= index {
                                index = record.state.output_index + 1;
                            }
                        }
                    }
                }
            }
            let event = patch_envelope_item(index, slot);
            let record_index = self.resolve(&event, slot)?;
            seen.insert(record_index);
            if self.records[record_index].patch {
                let pending = std::mem::take(&mut self.records[record_index].pending);
                for source in pending {
                    preceding.extend(self.patch_event(&source, record_index)?);
                }
                preceding.extend(self.patch_event(&event, record_index)?);
                let input = self.records[record_index].state.decoder.input().to_owned();
                let restored = self.restore_item(slot.clone(), record_index, &input, false);
                current = set_raw_path(&current, &format!("{path}.{position}"), &restored);
            } else if type_name == "function_call"
                && self
                    .tools
                    .get(&self.records[record_index].qualified)
                    .is_some_and(|tool| !tool.namespace.is_empty())
            {
                let restored = self.restore_item(slot.clone(), record_index, "", false);
                current = set_raw_path(&current, &format!("{path}.{position}"), &restored);
            }
            let slot_path = format!("{path}.{position}");
            let stored = gjson::get(std::str::from_utf8(&current).unwrap_or(""), &slot_path)
                .json()
                .as_bytes()
                .to_vec();
            items.push(stored);
        }
        for record_index in 0..self.records.len() {
            if seen.contains(&record_index)
                || (!self.records[record_index].patch && !self.converted)
            {
                continue;
            }
            if self.records[record_index].input_done && !self.records[record_index].item_done {
                let seed = br#"{"type":"function_call","status":"completed"}"#.to_vec();
                let input = self.records[record_index].state.decoder.input().to_owned();
                let restored = self.restore_item(seed, record_index, &input, false);
                self.records[record_index].completed_item = restored.clone();
                preceding.push(self.item_event(
                    "response.output_item.done",
                    &restored,
                    record_index,
                ));
                self.records[record_index].item_done = true;
            }
            if self.records[record_index].item_done {
                let output_index = self.records[record_index].state.output_index;
                let insert_at = if output_index < 0 || output_index as usize > items.len() {
                    items.len()
                } else {
                    output_index as usize
                };
                let completed = self.records[record_index].completed_item.clone();
                items.insert(insert_at, completed);
            }
        }
        if items.len() != original_len {
            current = set_raw_path(&current, path, &join_raw_array(&items));
        }
        if stream {
            self.finish()?;
            for record in &mut self.records {
                preceding.extend(std::mem::take(&mut record.pending));
            }
            if self.converted {
                let sequence = self.next_sequence();
                current = set_json_i64(&current, "sequence_number", sequence);
            }
        }
        Ok((current, preceding))
    }

    /// Converts one payload. A failure is terminal and is emitted only once.
    pub(crate) fn transform(&mut self, event: &[u8]) -> ApplyPatchTransform {
        if self.failed || self.terminal {
            return ApplyPatchTransform {
                events: Vec::new(),
                error: None,
            };
        }
        if !self.active {
            return ApplyPatchTransform {
                events: vec![event.to_vec()],
                error: None,
            };
        }
        let Ok(document) = std::str::from_utf8(event) else {
            return self.failure("invalid Responses event JSON");
        };
        let root = gjson::parse(document);
        let response_id = root.get("response.id").str().to_owned();
        if !response_id.is_empty() {
            self.response_id = response_id;
        }
        let incoming_sequence = root.get("sequence_number").i64();
        if incoming_sequence > self.sequence {
            self.sequence = incoming_sequence;
        }
        let kind = root.get("type").str().to_owned();
        let item_type = root.get("item.type").str().to_owned();
        let transformed = match kind.as_str() {
            "response.output_item.added"
            | "response.output_item.done"
            | "response.function_call_arguments.delta"
            | "response.function_call_arguments.done" => self.transform_item_event(event),
            "response.completed" | "response.incomplete" | "response.done" => {
                match self.envelope(event, true) {
                    Ok((body, mut preceding)) => {
                        preceding.push(body);
                        self.terminal = true;
                        Ok(preceding)
                    }
                    Err(error) => Err(error),
                }
            }
            "response.failed" => {
                self.terminal = true;
                Ok(vec![event.to_vec()])
            }
            _ => Ok(vec![event.to_vec()]),
        };
        let mut events = match transformed {
            Ok(events) => events,
            Err(error) => return self.failure(&error),
        };
        let native_custom =
            kind.starts_with("response.custom_tool_call_input.") || item_type == "custom_tool_call";
        let converted = self.converted;
        for event in &mut events {
            let mut sequence = std::str::from_utf8(event)
                .ok()
                .map(|text| gjson::get(text, "sequence_number").i64())
                .unwrap_or(0);
            if converted && !native_custom && sequence <= self.last_sequence {
                sequence = self.next_sequence();
                *event = set_json_i64(event, "sequence_number", sequence);
            }
            if sequence > self.last_sequence {
                self.last_sequence = sequence;
            }
        }
        ApplyPatchTransform {
            events,
            error: None,
        }
    }

    /// Accepts either a bare response or a terminal event envelope.
    pub(crate) fn transform_non_stream(&mut self, response: &[u8]) -> Result<Vec<u8>, String> {
        if !self.active {
            return Ok(response.to_vec());
        }
        match self.envelope(response, false) {
            Ok((body, _)) => Ok(body),
            Err(error) => {
                self.failed = true;
                self.errors.set_tool_input_error(Some(error.clone()));
                Err(error)
            }
        }
    }

    /// Rejects acquired calls whose final arguments have not been validated.
    pub(crate) fn finish(&self) -> Result<(), String> {
        if let Some(error) = self.errors.tool_input_error() {
            return Err(error.to_owned());
        }
        if self.terminal {
            return Ok(());
        }
        for record in &self.records {
            if record.patch && !record.input_done {
                return Err(
                    "incomplete apply_patch tool arguments received from upstream".to_owned(),
                );
            }
        }
        Ok(())
    }
}

fn patch_envelope_item(index: i64, item: &[u8]) -> Vec<u8> {
    let mut out = br#"{"type":"response.output_item.done","output_index":0,"item":{}}"#.to_vec();
    out = set_json_i64(&out, "output_index", index);
    set_raw_path(&out, "item", item)
}

#[cfg(test)]
mod tests {
    use super::{normalize_apply_patch_responses_request, ApplyPatchResponsesBridge};
    use crate::internal::client::codex::apply_patch::{go_json_string, unwrap_input, wrap_input};

    fn field(body: &[u8], path: &str) -> String {
        gjson::get(std::str::from_utf8(body).unwrap(), path)
            .str()
            .to_owned()
    }

    fn exists(body: &[u8], path: &str) -> bool {
        gjson::get(std::str::from_utf8(body).unwrap(), path).exists()
    }

    #[test]
    fn history_keeps_ordinary_declarations_and_converts_custom_patch_calls() {
        let history = r#"{"type":"custom_tool_call","call_id":"old","name":"apply_patch","input":"{\"input\":\"raw\"}"},{"type":"custom_tool_call_output","call_id":"old","output":"ok"},{"type":"function_call","call_id":"fn","name":"apply_patch","arguments":"{\"input\":\"existing\"}"}"#;
        for tools in [
            r#"{"tools":[{"type":"custom","name":"apply_patch"}],"input":[]}"#,
            r#"{"tools":[{"type":"function","name":"apply_patch"}],"input":[]}"#,
            r#"{"input":[]}"#,
        ] {
            let raw = tools.replace(r#""input":[]"#, &format!(r#""input":[{history}]"#));
            let out = normalize_apply_patch_responses_request(raw.as_bytes(), None).unwrap();
            assert_eq!(
                field(&out, "input.0.arguments"),
                wrap_input(r#"{"input":"raw"}"#)
            );
            assert_eq!(field(&out, "input.0.type"), "function_call");
            assert!(!exists(&out, "input.0.input"));
            assert_eq!(field(&out, "input.1.type"), "function_call_output");
            assert_eq!(field(&out, "input.2.arguments"), r#"{"input":"existing"}"#);
            if tools.contains(r#""type":"function""#) {
                assert!(!exists(&out, "tools.0.parameters"));
            }
            if tools.contains(r#""type":"custom""#) {
                assert_eq!(field(&out, "tools.0.type"), "function");
                assert_eq!(
                    field(&out, "tools.0.parameters.properties.input.type"),
                    "string"
                );
            }
        }
    }

    #[test]
    fn direct_function_wins_over_a_later_custom_declaration() {
        let additional = br#"{"tools":[{"type":"function","name":"apply_patch","description":"ordinary"}],"input":[{"type":"additional_tools","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
        let out = normalize_apply_patch_responses_request(additional, None).unwrap();
        assert_eq!(field(&out, "tools.0.type"), "function");
        assert!(!exists(&out, "tools.0.parameters"));
        assert!(!exists(&out, "input.0.tools.0"));

        let namespace = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"}]},{"type":"function","name":"n__apply_patch","description":"ordinary"}]}"#;
        let out = normalize_apply_patch_responses_request(namespace, None).unwrap();
        assert!(!exists(&out, "tools.0.tools.0"));
        assert_eq!(field(&out, "tools.1.type"), "function");
        assert!(!exists(&out, "tools.1.parameters"));
    }

    #[test]
    fn chat_function_preference_drops_the_translated_custom_tool() {
        let original = br#"{"tools":[{"type":"custom","name":"apply_patch"},{"type":"function","function":{"name":"apply_patch","parameters":{"type":"object","properties":{"x":{"type":"integer"}}}}}]}"#;
        let body = br#"{"tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"apply_patch","parameters":{"type":"object","properties":{"x":{"type":"integer"}}}}]}"#;
        let out = normalize_apply_patch_responses_request(body, Some(original)).unwrap();
        assert!(exists(&out, "tools.0.parameters.properties.x"));
        assert!(!exists(&out, "tools.0.parameters.properties.input"));
        assert!(!exists(&out, "tools.1"));
    }

    #[test]
    fn custom_tool_choice_follows_the_winning_declaration() {
        let raw = br#"{"tools":[{"type":"custom","name":"apply_patch"}],"tool_choice":{"type":"custom","name":"apply_patch","tools":[{"type":"custom","name":"apply_patch"}]}}"#;
        let out = normalize_apply_patch_responses_request(raw, None).unwrap();
        assert_eq!(field(&out, "tool_choice.type"), "function");
        assert_eq!(field(&out, "tool_choice.tools.0.type"), "function");
    }

    #[test]
    fn rejects_invalid_json_and_non_string_patch_history() {
        assert_eq!(
            normalize_apply_patch_responses_request(b"not-json", None),
            Err("invalid Responses request JSON")
        );
        let raw =
            br#"{"input":[{"type":"custom_tool_call","name":"apply_patch","input":{"x":1}}]}"#;
        assert_eq!(
            normalize_apply_patch_responses_request(raw, None),
            Err("apply_patch history input must be a string")
        );
    }

    const PATCH_REQUEST: &[u8] = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#;

    fn patch_item(kind: &str, id: &str, call: &str, name: &str, args: &str) -> String {
        format!(
            r#"{{"type":{},"id":{},"call_id":{},"name":{},"arguments":{}}}"#,
            go_json_string(kind),
            go_json_string(id),
            go_json_string(call),
            go_json_string(name),
            go_json_string(args),
        )
    }

    fn patch_event(kind: &str, index: i64, item: &str) -> Vec<u8> {
        format!(
            r#"{{"type":{},"output_index":{},"item":{}}}"#,
            go_json_string(kind),
            index,
            item
        )
        .into_bytes()
    }

    fn event_field(event: &[u8], path: &str) -> String {
        gjson::get(std::str::from_utf8(event).unwrap(), path)
            .str()
            .to_owned()
    }

    fn event_exists(event: &[u8], path: &str) -> bool {
        gjson::get(std::str::from_utf8(event).unwrap(), path).exists()
    }

    fn event_i64(event: &[u8], path: &str) -> i64 {
        gjson::get(std::str::from_utf8(event).unwrap(), path).i64()
    }

    fn send(bridge: &mut ApplyPatchResponsesBridge, event: &[u8]) -> Vec<Vec<u8>> {
        let transformed = bridge.transform(event);
        assert!(
            transformed.error.is_none(),
            "transform {}: {}",
            transformed.error.as_deref().unwrap_or(""),
            String::from_utf8_lossy(event)
        );
        transformed.events
    }

    fn assert_failed(bridge: &mut ApplyPatchResponsesBridge, event: &[u8]) {
        let transformed = bridge.transform(event);
        assert!(
            transformed.error.is_some(),
            "accepted {}",
            String::from_utf8_lossy(event)
        );
        assert_eq!(transformed.events.len(), 1);
        assert_eq!(
            event_field(&transformed.events[0], "type"),
            "response.failed"
        );
        assert!(bridge.tool_input_error().is_some());
    }

    fn streamed_patch_arguments() -> String {
        let mut args = String::from(r#"{"input":"*** Begin Patch"#);
        args.push('\\');
        args.push_str("n+中文");
        args.push('\\');
        args.push_str("uD83D");
        args.push('\\');
        args.push_str("uDE00 ");
        args.push('\\');
        args.push('"');
        args.push('\\');
        args.push('\\');
        args.push('\\');
        args.push_str("n*** End Patch");
        args.push('\\');
        args.push_str("n\"}");
        args
    }

    fn static_kind(kind: &str) -> &'static str {
        match kind {
            "response.custom_tool_call_input.done" => "response.custom_tool_call_input.done",
            "response.output_item.done" => "response.output_item.done",
            "response.completed" => "response.completed",
            "response.output_item.added" => "response.output_item.added",
            "response.custom_tool_call_input.delta" => "response.custom_tool_call_input.delta",
            _ => "other",
        }
    }

    #[test]
    fn bridge_delta_and_four_completions_preserve_split_source() {
        let args = streamed_patch_arguments();
        assert_eq!(args.len(), 70);
        let parts = [&args[..18], &args[18..35], &args[35..41], &args[41..]];
        let want = unwrap_input(&args).unwrap();
        for late in [false, true] {
            let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
            let name = if late { "" } else { "apply_patch" };
            let mut events = send(
                &mut bridge,
                &patch_event(
                    "response.output_item.added",
                    0,
                    &patch_item("function_call", "", "", name, ""),
                ),
            );
            for part in parts {
                let delta = format!(
                    r#"{{"type":"response.function_call_arguments.delta","output_index":0,"delta":{}}}"#,
                    go_json_string(part)
                );
                events.extend(send(&mut bridge, delta.as_bytes()));
            }
            let item = patch_item("function_call", "fc1", "c1", "apply_patch", &args);
            events.extend(send(
                &mut bridge,
                &patch_event("response.output_item.done", 0, &item),
            ));
            let completed = format!(
                r#"{{"type":"response.completed","response":{{"id":"r1","output":[{item}]}}}}"#
            );
            events.extend(send(&mut bridge, completed.as_bytes()));
            let mut delta = String::new();
            let mut counts = std::collections::HashMap::<&str, usize>::new();
            let mut last_sequence = -1i64;
            for event in &events {
                let kind = event_field(event, "type");
                *counts.entry(static_kind(&kind)).or_default() += 1;
                let sequence = event_i64(event, "sequence_number");
                assert!(
                    sequence > last_sequence,
                    "{}",
                    String::from_utf8_lossy(event)
                );
                last_sequence = sequence;
                match kind.as_str() {
                    "response.custom_tool_call_input.delta" => {
                        delta.push_str(&event_field(event, "delta"));
                    }
                    "response.custom_tool_call_input.done" => {
                        assert_eq!(event_field(event, "input"), want);
                        assert_eq!(event_field(event, "item_id"), "fc1");
                        assert_eq!(event_field(event, "call_id"), "c1");
                    }
                    "response.output_item.done" => {
                        assert_eq!(event_field(event, "item.input"), want);
                        assert!(!event_exists(event, "item.arguments"));
                    }
                    "response.completed" => {
                        assert_eq!(event_field(event, "response.output.0.input"), want);
                    }
                    _ => {}
                }
            }
            assert_eq!(delta, want);
            assert_eq!(
                counts
                    .get("response.custom_tool_call_input.done")
                    .copied()
                    .unwrap_or(0),
                1
            );
            assert_eq!(
                counts
                    .get("response.output_item.done")
                    .copied()
                    .unwrap_or(0),
                1
            );
            assert_eq!(counts.get("response.completed").copied().unwrap_or(0), 1);
            assert!(bridge.finish().is_ok());
            let duplicate =
                bridge.transform(br#"{"type":"response.completed","response":{"output":[]}}"#);
            assert!(duplicate.error.is_none());
            assert!(duplicate.events.is_empty());
        }
    }

    #[test]
    fn bridge_passes_native_and_ordinary_events_through_unchanged() {
        let requests: [&[u8]; 3] = [
            PATCH_REQUEST,
            br#"{"tools":[{"type":"function","name":"apply_patch"}]}"#,
            b"{}",
        ];
        for request in requests {
            let mut bridge = ApplyPatchResponsesBridge::new(request);
            let ordinary = patch_event(
                "response.output_item.done",
                1,
                &patch_item(
                    "function_call",
                    "ordinary",
                    "ordinary",
                    "lookup",
                    r#"{"x":1}"#,
                ),
            );
            let samples: [&[u8]; 3] = [
                br#"{ "type":"response.output_item.added", "output_index":0,"item":{"type":"custom_tool_call","id":"native","name":"apply_patch","input":""}}"#,
                br#"{ "type":"response.custom_tool_call_input.delta", "item_id":"native","delta":"raw patch"}"#,
                &ordinary,
            ];
            for event in samples {
                let out = send(&mut bridge, event);
                assert_eq!(out.len(), 1);
                assert_eq!(out[0], event);
            }
        }
    }

    #[test]
    fn bridge_retains_identity_and_snapshot_evidence() {
        let run = |before: &[Vec<u8>], after: &[u8]| {
            let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
            for event in before {
                send(&mut bridge, event);
            }
            assert_failed(&mut bridge, after);
            let more =
                bridge.transform(br#"{"type":"response.completed","response":{"output":[]}}"#);
            assert!(more.error.is_none());
            assert!(more.events.is_empty());
        };
        run(
            &[
                patch_event(
                    "response.output_item.added",
                    0,
                    &patch_item("function_call", "a", "ca", "apply_patch", ""),
                ),
                patch_event(
                    "response.output_item.added",
                    1,
                    &patch_item("function_call", "b", "cb", "lookup", ""),
                ),
            ],
            br#"{"type":"response.function_call_arguments.delta","output_index":1,"item_id":"a","delta":"{}"}"#,
        );
        run(
            &[patch_event(
                "response.output_item.added",
                0,
                &patch_item("function_call", "a", "ca", "apply_patch", ""),
            )],
            br#"{"type":"response.function_call_arguments.done","output_index":0,"item_id":"a","call_id":"other","arguments":"{\"input\":\"p\"}"}"#,
        );
        run(
            &[patch_event(
                "response.output_item.added",
                0,
                &patch_item("message", "a", "ca", "", ""),
            )],
            &patch_event(
                "response.output_item.done",
                0,
                &patch_item(
                    "function_call",
                    "a",
                    "ca",
                    "apply_patch",
                    r#"{"input":"p"}"#,
                ),
            ),
        );
        run(
            &[patch_event(
                "response.output_item.added",
                0,
                &patch_item("function_call", "a", "ca", "", r#"{"input":"p","extra":1}"#),
            )],
            &patch_event(
                "response.output_item.done",
                0,
                &patch_item(
                    "function_call",
                    "a",
                    "ca",
                    "apply_patch",
                    r#"{"input":"p"}"#,
                ),
            ),
        );
        run(
            &[],
            &patch_event(
                "response.output_item.added",
                0,
                &patch_item("function_call", "a", "ca", "apply_patch", r#"{"input":"p"#),
            ),
        );
        run(
            &[],
            br#"{"type":"response.completed","response":{"output":[{"type":"function_call","name":"apply_patch","arguments":"{}"}]}}"#
        );
        run(
            &[patch_event(
                "response.output_item.added",
                0,
                &patch_item("function_call", "a", "ca", "apply_patch", ""),
            )],
            br#"{"type":"response.completed","response":{"output":[{"type":"message","id":"a","content":[]}]}}"#
        );
        run(
            &[patch_event(
                "response.output_item.added",
                0,
                &patch_item("function_call", "a", "ca", "", ""),
            )],
            &patch_event(
                "response.output_item.done",
                0,
                &patch_item(
                    "function_call",
                    "a",
                    "changed",
                    "apply_patch",
                    r#"{"input":"p"}"#,
                ),
            ),
        );
    }

    #[test]
    fn bridge_stages_reject_incomplete_arguments_and_duplicate_done() {
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        send(
            &mut bridge,
            &patch_event(
                "response.output_item.added",
                0,
                &patch_item("function_call", "a", "ca", "apply_patch", ""),
            ),
        );
        assert_eq!(
            bridge.finish().unwrap_err(),
            "incomplete apply_patch tool arguments received from upstream"
        );

        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        for index in 0..2 {
            let id = format!("a{index}");
            let call = format!("c{index}");
            send(
                &mut bridge,
                &patch_event(
                    "response.output_item.added",
                    index,
                    &patch_item("function_call", &id, &call, "apply_patch", ""),
                ),
            );
            let args = wrap_input(&format!("patch{index}"));
            let done = format!(
                r#"{{"type":"response.function_call_arguments.done","output_index":{index},"arguments":{}}}"#,
                go_json_string(&args)
            );
            let out = send(&mut bridge, done.as_bytes());
            assert_eq!(out.len(), 1, "{}", String::from_utf8_lossy(&out.concat()));
            let item = patch_item("function_call", &id, &call, "apply_patch", &args);
            let out = send(
                &mut bridge,
                &patch_event("response.output_item.done", index, &item),
            );
            assert_eq!(out.len(), 1);
            assert_eq!(event_field(&out[0], "item.input"), format!("patch{index}"));
            let out = send(
                &mut bridge,
                &patch_event("response.output_item.done", index, &item),
            );
            assert!(out.is_empty(), "{}", String::from_utf8_lossy(&out.concat()));
        }
        assert!(bridge.finish().is_ok());
    }

    #[test]
    fn losing_custom_declaration_does_not_steal_ordinary_identity() {
        let direct = br#"{"tools":[{"type":"function","name":"apply_patch","description":"ordinary"}],"input":[{"type":"additional_tools","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
        let namespace = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"}]},{"type":"function","name":"n__apply_patch","description":"ordinary"}]}"#;
        for (request, name) in [
            (direct.as_slice(), "apply_patch"),
            (namespace.as_slice(), "n__apply_patch"),
        ] {
            normalize_apply_patch_responses_request(request, None).unwrap();
            let mut bridge = ApplyPatchResponsesBridge::new(request);
            let got = send(
                &mut bridge,
                &patch_event(
                    "response.output_item.done",
                    0,
                    &patch_item("function_call", "a", "c", name, r#"{"x":1}"#),
                ),
            );
            assert_eq!(got.len(), 1);
            assert_eq!(event_field(&got[0], "item.type"), "function_call");
        }
    }

    #[test]
    fn namespace_non_stream_restores_local_names() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"lookup"}]}]}"#;
        let mut bridge = ApplyPatchResponsesBridge::new(request);
        let raw = format!(
            r#"{{"id":"r","output":[{},{}]}}"#,
            patch_item(
                "function_call",
                "a",
                "c",
                "n__apply_patch",
                &wrap_input("p")
            ),
            patch_item("function_call", "b", "d", "n__lookup", r#"{"x":1}"#)
        );
        let out = bridge.transform_non_stream(raw.as_bytes()).unwrap();
        assert_eq!(field(&out, "output.0.type"), "custom_tool_call");
        assert_eq!(field(&out, "output.0.name"), "apply_patch");
        assert_eq!(field(&out, "output.0.namespace"), "n");
        assert_eq!(field(&out, "output.1.name"), "lookup");
        assert_eq!(field(&out, "output.1.namespace"), "n");
        assert_eq!(field(&out, "output.1.arguments"), r#"{"x":1}"#);
    }

    #[test]
    fn continuation_keeps_streamed_input_when_the_terminal_item_omits_arguments() {
        for source in ["deltas", "arguments.done"] {
            let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
            send(
                &mut bridge,
                &patch_event(
                    "response.output_item.added",
                    0,
                    &patch_item("function_call", "a", "c", "apply_patch", ""),
                ),
            );
            let wrapped = wrap_input("p");
            let event = if source == "deltas" {
                format!(
                    r#"{{"type":"response.function_call_arguments.delta","item_id":"a","delta":{}}}"#,
                    go_json_string(&wrapped)
                )
            } else {
                format!(
                    r#"{{"type":"response.function_call_arguments.done","item_id":"a","arguments":{}}}"#,
                    go_json_string(&wrapped)
                )
            };
            send(&mut bridge, event.as_bytes());
            let out = send(
                &mut bridge,
                br#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"a","call_id":"c","name":"apply_patch"}}"#,
            );
            assert_eq!(event_field(out.last().unwrap(), "item.input"), "p");
            let out = send(
                &mut bridge,
                br#"{"type":"response.completed","response":{"output":[]}}"#,
            );
            assert_eq!(
                event_field(out.last().unwrap(), "response.output.0.input"),
                "p"
            );
        }
    }

    #[test]
    fn native_terminal_events_keep_their_bytes() {
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        let events: [&[u8]; 2] = [
            br#"{ "type":"response.output_item.done", "output_index":0, "item":{"type":"custom_tool_call","id":"n","name":"apply_patch","input":"raw"}}"#,
            br#"{ "type":"response.completed", "sequence_number":71, "response": {"output":[{"type":"custom_tool_call","id":"n","name":"apply_patch","input":"raw"}]}}"#,
        ];
        for event in events {
            let out = send(&mut bridge, event);
            assert_eq!(out, vec![event.to_vec()]);
        }
    }

    #[test]
    fn root_late_name_completes_the_custom_input() {
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        send(
            &mut bridge,
            br#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a"}}"#,
        );
        let out = send(
            &mut bridge,
            br#"{"type":"response.function_call_arguments.done","output_index":0,"item_id":"a","call_id":"c","name":"apply_patch","arguments":"{\"input\":\"p\"}"}"#,
        );
        assert!(!out.is_empty());
        assert_eq!(
            event_field(out.last().unwrap(), "type"),
            "response.custom_tool_call_input.done"
        );
        assert!(bridge.finish().is_ok());
    }

    #[test]
    fn final_snapshot_does_not_invent_a_preview() {
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        let out = send(
            &mut bridge,
            &patch_event(
                "response.output_item.done",
                0,
                &patch_item("function_call", "a", "c", "apply_patch", &wrap_input("p")),
            ),
        );
        assert!(out
            .iter()
            .all(|event| event_field(event, "type") != "response.custom_tool_call_input.delta"));
    }

    #[test]
    fn every_matched_key_can_select_patch_provenance() {
        let keys = ["output_index", "item_id", "call_id"];
        for patch_key in keys {
            for first_key in keys {
                if first_key == patch_key {
                    continue;
                }
                let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
                send(
                    &mut bridge,
                    &patch_event(
                        "response.output_item.added",
                        0,
                        &patch_item("function_call", "a", "ca", "lookup", ""),
                    ),
                );
                send(
                    &mut bridge,
                    &patch_event(
                        "response.output_item.added",
                        1,
                        &patch_item("function_call", "b", "cb", "apply_patch", ""),
                    ),
                );
                let mut output_index = "0";
                let mut item_id = r#""a""#;
                let mut call_id = r#""ca""#;
                match patch_key {
                    "output_index" => output_index = "1",
                    "item_id" => item_id = r#""b""#,
                    "call_id" => call_id = r#""cb""#,
                    _ => unreachable!(),
                }
                let event = format!(
                    r#"{{"type":"response.function_call_arguments.delta","output_index":{output_index},"item_id":{item_id},"call_id":{call_id},"delta":"{{}}"}}"#
                );
                assert_failed(&mut bridge, event.as_bytes());
            }
        }
        for discover in 0..3 {
            let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
            for index in 0..3 {
                send(
                    &mut bridge,
                    &patch_event(
                        "response.output_item.added",
                        index,
                        &patch_item(
                            "function_call",
                            &format!("i{index}"),
                            &format!("c{index}"),
                            "",
                            "",
                        ),
                    ),
                );
            }
            send(
                &mut bridge,
                br#"{"type":"response.function_call_arguments.delta","output_index":0,"item_id":"i1","call_id":"c2","delta":""}"#,
            );
            let done = patch_event(
                "response.output_item.done",
                discover,
                &patch_item(
                    "function_call",
                    &format!("i{discover}"),
                    &format!("c{discover}"),
                    "apply_patch",
                    &wrap_input("p"),
                ),
            );
            let transformed = bridge.transform(&done);
            assert!(transformed.error.is_some(), "discover {discover}");
            assert_eq!(transformed.events.len(), 1);
        }
    }

    #[test]
    fn completed_window_rejects_a_later_snapshot_until_the_response_closes() {
        let item = patch_item("function_call", "a", "c", "apply_patch", &wrap_input("p"));
        for terminal in [false, true] {
            let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
            send(
                &mut bridge,
                &patch_event("response.output_item.done", 0, &item),
            );
            if terminal {
                let completed =
                    format!(r#"{{"type":"response.completed","response":{{"output":[{item}]}}}}"#);
                send(&mut bridge, completed.as_bytes());
            }
            let changed = patch_event(
                "response.output_item.done",
                0,
                &patch_item(
                    "function_call",
                    "a",
                    "c",
                    "apply_patch",
                    &wrap_input("different"),
                ),
            );
            let transformed = bridge.transform(&changed);
            if terminal {
                assert!(transformed.error.is_none());
                assert!(transformed.events.is_empty());
            } else {
                assert!(transformed.error.is_some());
            }
        }
    }

    #[test]
    fn omitted_completed_items_keep_patch_and_ordinary_output() {
        let ordinary =
            r#"{"type":"message","id":"m","content":[{"type":"output_text","text":"ok"}]}"#;
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        send(
            &mut bridge,
            &patch_event(
                "response.output_item.done",
                0,
                &patch_item("function_call", "p", "cp", "apply_patch", &wrap_input("p")),
            ),
        );
        send(
            &mut bridge,
            &patch_event("response.output_item.done", 1, ordinary),
        );
        let out = send(
            &mut bridge,
            br#"{"type":"response.completed","response":{"output":[]}}"#,
        );
        let final_event = out.last().unwrap();
        assert_eq!(event_field(final_event, "response.output.0.input"), "p");
        assert_eq!(event_field(final_event, "response.output.1.id"), "m");

        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        send(
            &mut bridge,
            &patch_event(
                "response.output_item.done",
                0,
                &patch_item("function_call", "p", "cp", "apply_patch", &wrap_input("p")),
            ),
        );
        let completed =
            format!(r#"{{"type":"response.completed","response":{{"output":[{ordinary}]}}}}"#);
        let out = send(&mut bridge, completed.as_bytes());
        let final_event = out.last().unwrap();
        assert_eq!(event_field(final_event, "response.output.0.input"), "p");
        assert_eq!(event_field(final_event, "response.output.1.id"), "m");
    }

    #[test]
    fn unmatched_identity_keys_stay_evidence_after_the_name_arrives() {
        let keys = [
            r#""item_id":"b""#,
            r#""call_id":"cb""#,
            r#""output_index":1"#,
        ];
        for known in [false, true] {
            for key in keys {
                let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
                if known {
                    send(
                        &mut bridge,
                        &patch_event(
                            "response.output_item.added",
                            0,
                            &patch_item("function_call", "a", "ca", "lookup", ""),
                        ),
                    );
                }
                send(
                    &mut bridge,
                    br#"{"type":"response.output_item.added","output_index":1,"item_id":"a","call_id":"ca","item":{"type":"function_call","id":"b","call_id":"cb","name":""}}"#,
                );
                let event = format!(
                    r#"{{"type":"response.function_call_arguments.done",{key},"name":"apply_patch","arguments":{}}}"#,
                    go_json_string(r#"{"input":"p"}"#)
                );
                assert_failed(&mut bridge, event.as_bytes());
            }
        }
    }

    #[test]
    fn mixed_patch_and_ordinary_events_keep_increasing_sequences() {
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        let raw_events = [
            r#"{"type":"response.output_item.added","sequence_number":1,"output_index":0,"item":{"type":"function_call","id":"a","call_id":"c","name":"apply_patch","arguments":""}}"#,
            r#"{"type":"response.output_item.added","sequence_number":2,"output_index":1,"item":{"type":"function_call","id":"b","name":"lookup","arguments":""}}"#,
            r#"{"type":"response.function_call_arguments.done","sequence_number":3,"output_index":0,"item_id":"a","arguments":"{\"input\":\"p\"}"}"#,
            r#"{"type":"response.output_item.done","sequence_number":4,"output_index":1,"item":{"type":"function_call","id":"b","name":"lookup","arguments":"{\"x\":1}"}}"#,
            r#"{"type":"response.completed","sequence_number":5,"response":{"output":[]}}"#,
        ];
        let mut events = Vec::new();
        for raw in raw_events {
            events.extend(send(&mut bridge, raw.as_bytes()));
        }
        let mut last = -1i64;
        for event in &events {
            let sequence = event_i64(event, "sequence_number");
            assert!(sequence > last, "{}", String::from_utf8_lossy(event));
            last = sequence;
        }
    }

    #[test]
    fn ordinary_root_name_passes_through() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"lookup"}]}]}"#;
        let mut bridge = ApplyPatchResponsesBridge::new(request);
        let raw = br#"{ "type":"response.function_call_arguments.done", "output_index":0,"name":"lookup","namespace":"n","arguments":"{}"}"#;
        let out = send(&mut bridge, raw);
        assert_eq!(out, vec![raw.to_vec()]);
    }

    #[test]
    fn native_custom_bytes_stay_outside_a_buffered_patch_call() {
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        send(
            &mut bridge,
            br#"{"type":"response.output_item.added","sequence_number":1,"output_index":0,"item":{"type":"function_call","id":"a","name":"apply_patch","arguments":""}}"#,
        );
        let native = br#"  { "type":"response.output_item.added", "sequence_number":2, "output_index":1,"item":{"type":"custom_tool_call","id":"n","name":"apply_patch","input":""}}  "#;
        let out = send(&mut bridge, native);
        assert_eq!(out, vec![native.to_vec()]);
    }

    #[test]
    fn named_late_identity_waits_for_both_ids() {
        for first in [
            "item",
            "call",
            "neither",
            "neither-item-first",
            "neither-call-first",
        ] {
            for boundary in ["item", "terminal"] {
                let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
                let mut id = "";
                let mut call = "";
                if first == "item" {
                    id = "a";
                }
                if first == "call" {
                    call = "c";
                }
                let pending = [
                    patch_event(
                        "response.output_item.added",
                        0,
                        &patch_item("function_call", id, call, "apply_patch", ""),
                    ),
                    format!(
                        r#"{{"type":"response.function_call_arguments.delta","output_index":0,"delta":{}}}"#,
                        go_json_string(r#"{"input":"p"#)
                    )
                    .into_bytes(),
                    format!(
                        r#"{{"type":"response.function_call_arguments.delta","output_index":0,"delta":{}}}"#,
                        go_json_string(r#"q"}"#)
                    )
                    .into_bytes(),
                    br#"{"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\"input\":\"pq\"}"}"#
                        .to_vec(),
                ];
                for event in &pending {
                    assert!(
                        send(&mut bridge, event).is_empty(),
                        "{first} {boundary} {}",
                        String::from_utf8_lossy(event)
                    );
                }
                if first.starts_with("neither-") {
                    let (id, call) = if first == "neither-call-first" {
                        ("", "c")
                    } else {
                        ("a", "")
                    };
                    assert!(send(
                        &mut bridge,
                        &patch_event(
                            "response.output_item.added",
                            0,
                            &patch_item("function_call", id, call, "apply_patch", ""),
                        ),
                    )
                    .is_empty());
                }
                let item = patch_item(
                    "function_call",
                    "a",
                    "c",
                    "apply_patch",
                    r#"{"input":"pq"}"#,
                );
                let mut events = Vec::new();
                if boundary == "item" {
                    events.extend(send(
                        &mut bridge,
                        &patch_event("response.output_item.done", 0, &item),
                    ));
                }
                let completed =
                    format!(r#"{{"type":"response.completed","response":{{"output":[{item}]}}}}"#);
                events.extend(send(&mut bridge, completed.as_bytes()));
                let mut counts = std::collections::HashMap::<&str, usize>::new();
                let mut fragments = Vec::new();
                for event in &events {
                    let kind = event_field(event, "type");
                    *counts.entry(static_kind(&kind)).or_default() += 1;
                    let (identity_id, identity_call, identity_input) = match kind.as_str() {
                        "response.output_item.added" | "response.output_item.done" => (
                            event_field(event, "item.id"),
                            event_field(event, "item.call_id"),
                            event_field(event, "item.input"),
                        ),
                        "response.completed" => (
                            event_field(event, "response.output.0.id"),
                            event_field(event, "response.output.0.call_id"),
                            event_field(event, "response.output.0.input"),
                        ),
                        _ => (
                            event_field(event, "item_id"),
                            event_field(event, "call_id"),
                            event_field(event, "input"),
                        ),
                    };
                    if kind == "response.output_item.added" {
                        assert!(
                            identity_input.is_empty(),
                            "{}",
                            String::from_utf8_lossy(event)
                        );
                    }
                    if kind == "response.output_item.done"
                        || kind == "response.custom_tool_call_input.done"
                        || kind == "response.completed"
                    {
                        assert_eq!(identity_input, "pq", "{}", String::from_utf8_lossy(event));
                    }
                    if kind != "response.completed" {
                        assert!(
                            event_exists(event, "output_index"),
                            "{}",
                            String::from_utf8_lossy(event)
                        );
                        assert_eq!(event_i64(event, "output_index"), 0);
                    }
                    assert_eq!(identity_id, "a", "{}", String::from_utf8_lossy(event));
                    assert_eq!(identity_call, "c", "{}", String::from_utf8_lossy(event));
                    if kind == "response.custom_tool_call_input.delta" {
                        fragments.push(event_field(event, "delta"));
                    }
                }
                assert_eq!(fragments.join("|"), "p|q", "{first} {boundary}");
                for kind in [
                    "response.output_item.added",
                    "response.custom_tool_call_input.done",
                    "response.output_item.done",
                    "response.completed",
                ] {
                    assert_eq!(
                        counts.get(kind).copied().unwrap_or(0),
                        1,
                        "{first} {boundary} {counts:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn named_late_identity_evidence_is_one_shot() {
        let cases = [
            (
                patch_event(
                    "response.output_item.added",
                    0,
                    &patch_item("function_call", "a", "", "apply_patch", ""),
                ),
                patch_event(
                    "response.output_item.done",
                    0,
                    &patch_item(
                        "function_call",
                        "changed",
                        "c",
                        "apply_patch",
                        r#"{"input":"pq"}"#,
                    ),
                ),
            ),
            (
                patch_event(
                    "response.output_item.added",
                    0,
                    &patch_item("function_call", "", "c", "apply_patch", ""),
                ),
                patch_event(
                    "response.output_item.done",
                    0,
                    &patch_item(
                        "function_call",
                        "a",
                        "changed",
                        "apply_patch",
                        r#"{"input":"pq"}"#,
                    ),
                ),
            ),
            (
                patch_event(
                    "response.output_item.added",
                    0,
                    &patch_item("function_call", "", "", "apply_patch", r#"{"input":"p"#),
                ),
                patch_event(
                    "response.output_item.done",
                    0,
                    &patch_item(
                        "function_call",
                        "a",
                        "c",
                        "apply_patch",
                        r#"{"input":"pq"}"#,
                    ),
                ),
            ),
            (
                patch_event(
                    "response.output_item.added",
                    0,
                    &patch_item(
                        "function_call",
                        "a",
                        "",
                        "apply_patch",
                        r#"{"input":"pq","extra":1}"#,
                    ),
                ),
                patch_event(
                    "response.output_item.done",
                    0,
                    &patch_item(
                        "function_call",
                        "a",
                        "c",
                        "apply_patch",
                        r#"{"input":"pq"}"#,
                    ),
                ),
            ),
            (
                patch_event(
                    "response.output_item.done",
                    0,
                    &patch_item("function_call", "a", "", "apply_patch", r#"{"input":"pq"}"#),
                ),
                br#"{"type":"response.completed","response":{"output":[]}}"#.to_vec(),
            ),
        ];
        for (before, after) in cases {
            let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
            let first = bridge.transform(&before);
            let failed = if first.error.is_some() {
                first
            } else {
                assert!(
                    first.events.is_empty(),
                    "{}",
                    String::from_utf8_lossy(&before)
                );
                bridge.transform(&after)
            };
            assert!(failed.error.is_some());
            assert_eq!(failed.events.len(), 1);
            assert_eq!(event_field(&failed.events[0], "type"), "response.failed");
            let again = bridge.transform(&after);
            assert!(again.error.is_none());
            assert!(again.events.is_empty());
        }
    }

    #[test]
    fn named_late_identity_interleaves_independent_calls() {
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        for (index, item) in [
            patch_item("function_call", "a0", "", "apply_patch", ""),
            patch_item("function_call", "", "c1", "apply_patch", ""),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(send(
                &mut bridge,
                &patch_event("response.output_item.added", index as i64, &item),
            )
            .is_empty());
            let delta = format!(
                r#"{{"type":"response.function_call_arguments.delta","output_index":{index},"delta":{}}}"#,
                go_json_string(&format!(r#"{{"input":"{index}"}}"#))
            );
            assert!(send(&mut bridge, delta.as_bytes()).is_empty());
        }
        let mut events = Vec::new();
        for index in [1i64, 0] {
            let item = patch_item(
                "function_call",
                &format!("a{index}"),
                &format!("c{index}"),
                "apply_patch",
                &format!(r#"{{"input":"{index}"}}"#),
            );
            events.extend(send(
                &mut bridge,
                &patch_event("response.output_item.done", index, &item),
            ));
        }
        events.extend(send(
            &mut bridge,
            br#"{"type":"response.completed","response":{"output":[]}}"#,
        ));
        for event in &events {
            if event_field(event, "type") == "response.completed" {
                for index in 0..2 {
                    let prefix = format!("response.output.{index}");
                    assert_eq!(
                        event_field(event, &format!("{prefix}.id")),
                        format!("a{index}")
                    );
                    assert_eq!(
                        event_field(event, &format!("{prefix}.call_id")),
                        format!("c{index}")
                    );
                    assert_eq!(
                        event_field(event, &format!("{prefix}.input")),
                        format!("{index}")
                    );
                }
                continue;
            }
            let index = event_i64(event, "output_index");
            let (id, call) = if event_exists(event, "item") {
                (
                    event_field(event, "item.id"),
                    event_field(event, "item.call_id"),
                )
            } else {
                (event_field(event, "item_id"), event_field(event, "call_id"))
            };
            assert_eq!(
                id,
                format!("a{index}"),
                "{}",
                String::from_utf8_lossy(event)
            );
            assert_eq!(
                call,
                format!("c{index}"),
                "{}",
                String::from_utf8_lossy(event)
            );
        }
    }

    #[test]
    fn check_identity_withholds_the_dispatcher_name() {
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        let event = br#"{"type":"response.output_item.added","output_index":0,"item_id":"a","item":{"type":"function_call","id":"b","call_id":"c","name":"apply_patch"}}"#;
        assert!(bridge.check_identity(event).is_ok());
        assert_failed(
            &mut bridge,
            br#"{"type":"response.function_call_arguments.done","item_id":"b","name":"apply_patch","arguments":"{\"input\":\"p\"}"}"#,
        );
    }

    #[test]
    fn fail_emits_one_terminal_payload() {
        let mut bridge = ApplyPatchResponsesBridge::new(PATCH_REQUEST);
        let failed = bridge.fail("boom");
        assert_eq!(failed.error.as_deref(), Some("boom"));
        assert_eq!(failed.events.len(), 1);
        assert_eq!(event_field(&failed.events[0], "type"), "response.failed");
        assert_eq!(bridge.tool_input_error(), Some("boom"));
        let again = bridge.fail("again");
        assert!(again.error.is_none());
        assert!(again.events.is_empty());
        assert_eq!(bridge.tool_input_error(), Some("boom"));
    }
}
