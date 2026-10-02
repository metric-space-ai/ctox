// ref: internal/translator/common/apply_patch_responses.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only

//! Adapts Responses `apply_patch` declarations and explicit custom history.
//!
//! Winners are collected before rewriting. The response-event bridge
//! (`ApplyPatchResponsesBridge`) is not in this module yet.

use std::collections::{HashMap, HashSet};

use gjson::{Kind, Value};

use crate::internal::client::codex::apply_patch::{
    description, go_json_string, is_custom_tool, parameters, wrap_input,
};

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

fn prefer_chat_function_patch_tools(original: &[u8], declarations: &[u8]) -> Vec<u8> {
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

fn qualify_namespace_tool_name(namespace_name: &str, child_name: &str) -> String {
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

fn set_json_string(data: &[u8], path: &str, value: &str) -> Vec<u8> {
    let literal = go_json_string(value);
    set_raw_path(data, path, literal.as_bytes())
}

fn set_raw_path(data: &[u8], path: &str, replacement: &[u8]) -> Vec<u8> {
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

#[cfg(test)]
mod tests {
    use super::normalize_apply_patch_responses_request;
    use crate::internal::client::codex::apply_patch::wrap_input;

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
}
