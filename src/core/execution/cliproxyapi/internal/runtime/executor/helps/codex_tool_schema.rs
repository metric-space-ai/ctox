// ref: internal/runtime/executor/helps/codex_tool_schema.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only

//! Normalizes Codex function-tool schemas before the upstream request.
//!
//! Pure constant unions of at least eight branches become enums. Unsupported
//! regex patterns are removed only from JSON Schema keyword locations.
//! `normalize_codex_tool_schemas` does not rewrite numeric field types.
//! `normalize_codex_tool_integer_types` does that separately, and only for a
//! Codex user agent whose target executor is not Codex itself.

use std::collections::{BTreeMap, HashSet};

use gjson::Kind;
use serde_json::Value;

use crate::internal::translator::common::set_raw_path;
use crate::internal::util::strip_unsupported_schema_patterns;

#[cfg(test)]
#[path = "codex_tool_integer_fields_v13_test.rs"]
mod candidate_integer_field_tests;

const CODEX_COMPLEX_UNION_BRANCH_THRESHOLD: usize = 8;

/// Simplifies Codex tool schemas in `body` and returns the original bytes when
/// nothing changes. Unchanged tools and every field outside `tools` keep their
/// original spelling.
#[must_use]
pub fn normalize_codex_tool_schemas(body: &[u8]) -> Vec<u8> {
    if body.is_empty() {
        return Vec::new();
    }
    let Ok(document) = std::str::from_utf8(body) else {
        return body.to_vec();
    };
    let Some((start, end)) = first_object_field_span(document, "tools") else {
        return body.to_vec();
    };
    let Some(updated) = normalize_tool_list_json(&document[start..end]) else {
        return body.to_vec();
    };
    let mut out = String::with_capacity(document.len() - (end - start) + updated.len());
    out.push_str(&document[..start]);
    out.push_str(&updated);
    out.push_str(&document[end..]);
    out.into_bytes()
}

/// Reports whether `headers` carries a User-Agent that identifies a Codex client.
#[must_use]
pub fn is_codex_user_agent(headers: &BTreeMap<String, Vec<String>>) -> bool {
    header_value_case_insensitive(headers, "User-Agent")
        .is_some_and(|value| value.to_ascii_lowercase().contains("codex"))
}

/// Codex and Codex WebSocket targets keep reserved tool schemas unchanged.
#[must_use]
pub fn is_codex_target_executor(target_executor: &str) -> bool {
    matches!(
        target_executor.trim().to_ascii_lowercase().as_str(),
        "codex" | "codex-websockets" | "codex_websockets"
    )
}

/// Rewrites selected Codex client tool fields from `number` to `integer`.
///
/// Empty bodies, non-Codex user agents, and payloads with no matching field
/// keep their original bytes. Only the explicitly selected type values change.
///
/// Upstream walks top-level `tools` and `input`. Antigravity's translated
/// envelope stores the same declarations at `request.tools`, which that
/// top-level walk never sees. The same element rules are applied there so the
/// body this executor sends carries integer types.
#[must_use]
pub fn normalize_codex_tool_integer_types(
    body: &[u8],
    headers: &BTreeMap<String, Vec<String>>,
) -> Vec<u8> {
    if body.is_empty() || !is_codex_user_agent(headers) {
        return body.to_vec();
    }
    let Ok(document) = std::str::from_utf8(body) else {
        return body.to_vec();
    };
    let mut current = document.to_owned();
    let mut changed = false;
    if let Some(updated) = replace_top_level_array(&current, "tools", splice_integer_tool_list) {
        current = updated;
        changed = true;
    }
    if let Some(updated) = replace_top_level_array(&current, "input", splice_input_additional_tools)
    {
        current = updated;
        changed = true;
    }
    if let Some(updated) =
        replace_nested_object_array(&current, "request", "tools", splice_integer_tool_list)
    {
        current = updated;
        changed = true;
    }
    if changed {
        current.into_bytes()
    } else {
        body.to_vec()
    }
}

/// Applies integer normalization unless `target_executor` is a Codex executor.
#[must_use]
pub fn normalize_codex_tool_integer_types_for_executor(
    body: &[u8],
    headers: &BTreeMap<String, Vec<String>>,
    target_executor: &str,
) -> Vec<u8> {
    if is_codex_target_executor(target_executor) {
        return body.to_vec();
    }
    normalize_codex_tool_integer_types(body, headers)
}

fn header_value_case_insensitive<'a>(
    headers: &'a BTreeMap<String, Vec<String>>,
    name: &str,
) -> Option<&'a str> {
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

fn replace_top_level_array(
    document: &str,
    field: &str,
    transform: fn(&str) -> Option<String>,
) -> Option<String> {
    let (start, end) = first_object_field_span(document, field)?;
    let updated = transform(&document[start..end])?;
    splice_span(document, start, end, &updated)
}

fn replace_nested_object_array(
    document: &str,
    object_field: &str,
    array_field: &str,
    transform: fn(&str) -> Option<String>,
) -> Option<String> {
    let (start, end) = first_object_field_span(document, object_field)?;
    let nested = &document[start..end];
    let (nested_start, nested_end) = first_object_field_span(nested, array_field)?;
    let updated = transform(&nested[nested_start..nested_end])?;
    splice_span(document, start + nested_start, start + nested_end, &updated)
}

fn splice_span(document: &str, start: usize, end: usize, updated: &str) -> Option<String> {
    let mut out = String::with_capacity(document.len() - (end - start) + updated.len());
    out.push_str(&document[..start]);
    out.push_str(updated);
    out.push_str(&document[end..]);
    Some(out)
}

fn splice_integer_tool_list(tools_json: &str) -> Option<String> {
    splice_json_array(tools_json, normalize_integer_tool_json)
}

fn splice_input_additional_tools(input_json: &str) -> Option<String> {
    splice_json_array(input_json, normalize_additional_tools_item)
}

fn splice_json_array(
    array_json: &str,
    transform: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let items = gjson::parse(array_json);
    if items.kind() != Kind::Array {
        return None;
    }
    let raw = items.json();
    let mut scan = 0;
    let mut splice_from = 0;
    let mut out = String::new();
    let mut changed = false;
    let mut failed = false;
    items.each(|_, item| {
        if failed {
            return false;
        }
        let item_raw = item.json();
        if item_raw.is_empty() {
            failed = true;
            return false;
        }
        let Some(relative) = raw[scan..].find(item_raw) else {
            failed = true;
            return false;
        };
        let start = scan + relative;
        scan = start + item_raw.len();
        if let Some(updated) = transform(item_raw) {
            out.push_str(&raw[splice_from..start]);
            out.push_str(&updated);
            splice_from = scan;
            changed = true;
        }
        true
    });
    if failed || !changed {
        return None;
    }
    out.push_str(&raw[splice_from..]);
    Some(out)
}

fn normalize_additional_tools_item(item_json: &str) -> Option<String> {
    let item = gjson::parse(item_json);
    let kind = {
        let kind_value = item.get("type");
        kind_value.str().to_owned()
    };
    if kind != "additional_tools" {
        return None;
    }
    let tools = item.get("tools");
    if tools.kind() != Kind::Array {
        return None;
    }
    let updated = splice_integer_tool_list(tools.json())?;
    replace_object_field(item_json, "tools", &updated)
}

fn normalize_integer_tool_json(tool_json: &str) -> Option<String> {
    normalize_integer_tool_json_with_namespace(tool_json, "")
}

/// ref: internal/client/codex/tool-schema/tool_schema.go:288-366 @ d7914afd
fn normalize_integer_tool_json_with_namespace(tool_json: &str, namespace: &str) -> Option<String> {
    let tool = gjson::parse(tool_json);
    if tool.get("type").str() == "namespace" {
        // Upstream accepts one explicit namespace level only.
        if !namespace.is_empty() {
            return None;
        }
        let name = tool.get("name").str().to_owned();
        if name.is_empty() {
            return None;
        }
        let nested = tool.get("tools");
        let updated = splice_json_array(nested.json(), |child| {
            normalize_integer_tool_json_with_namespace(child, &name)
        })?;
        return replace_object_field(tool_json, "tools", &updated);
    }
    for key in ["function_declarations", "functionDeclarations"] {
        let declarations = tool.get(key);
        if declarations.kind() == Kind::Array {
            let updated = splice_json_array(declarations.json(), |child| {
                normalize_integer_tool_json_with_namespace(child, namespace)
            })?;
            return replace_object_field(tool_json, key, &updated);
        }
    }

    let mut name = tool.get("name").str().to_owned();
    let mut parameter_path = "parameters";
    let mut parameters = tool.get(parameter_path);
    if parameters.kind() != Kind::Object {
        let function_parameters = tool.get("function.parameters");
        let input_schema = tool.get("input_schema");
        let json_schema = tool.get("parametersJsonSchema");
        if function_parameters.kind() == Kind::Object {
            parameter_path = "function.parameters";
            parameters = function_parameters;
            if name.is_empty() {
                name = tool.get("function.name").str().to_owned();
            }
        } else if input_schema.kind() == Kind::Object {
            parameter_path = "input_schema";
            parameters = input_schema;
        } else if json_schema.kind() == Kind::Object {
            parameter_path = "parametersJsonSchema";
            parameters = json_schema;
        } else {
            return None;
        }
    }
    if !namespace.is_empty() {
        name = format!("{namespace}__{name}");
    }
    let updated_parameters =
        normalize_raw_integer_field_types(parameters.json(), codex_integer_fields(&name))?;
    let updated = set_raw_path(
        tool_json.as_bytes(),
        parameter_path,
        updated_parameters.as_bytes(),
    );
    (updated != tool_json.as_bytes())
        .then(|| String::from_utf8(updated).ok())
        .flatten()
}

/// Keys are explicit paths relative to parameters.properties, never recursive
/// field-name matches.
/// ref: internal/client/codex/tool-schema/tool_schema.go:41-164 @ d7914afd
fn codex_integer_fields(tool_name: &str) -> &'static [&'static str] {
    let base = tool_name.trim();
    let base = base
        .strip_prefix("functions__")
        .or_else(|| base.strip_prefix("collab__"))
        .unwrap_or(base);
    let base = match base {
        "multi_agent_v1__wait_agent" | "collaboration__wait_agent" => "wait_agent",
        "collaboration__get_channels"
        | "collaboration__list_threads"
        | "collaboration__search_posts"
        | "collaboration__read_thread"
        | "collaboration__read_post" => base.strip_prefix("collaboration__").unwrap_or(base),
        _ => base,
    };
    match base {
        "exec_command" => &["yield_time_ms", "max_output_tokens", "timeout_ms"],
        "write_stdin" => &["session_id", "yield_time_ms", "max_output_tokens"],
        "sleep" => &["duration_ms"],
        "wait_agent" => &["timeout_ms"],
        "wait" => &["yield_time_ms", "max_tokens"],
        "tool_search" => &["limit"],
        "test_sync_tool" => &[
            "sleep_before_ms",
            "sleep_after_ms",
            "participants",
            "timeout_ms",
            "barrier.properties.participants",
            "barrier.properties.timeout_ms",
        ],
        "create_goal" => &["token_budget"],
        "get_channels" => &["limit"],
        "list_threads" | "search_posts" | "read_thread" => &["limit", "max_chars_per_post"],
        "read_post" | "history__read_item" => &["offset_chars", "limit_chars"],
        "memories__list" | "notes__list_files_by_prefix" => &["max_results"],
        "memories__read" => &["line_offset", "max_lines"],
        "memories__search" => &["context_lines", "max_results"],
        "history__list_windows" | "history__search_contents" => &["limit"],
        "history__list_items" => &["limit", "max_chars_per_item"],
        "notes__read_file" => &[
            "start_line",
            "stop_line",
            "start_line.anyOf.0",
            "stop_line.anyOf.0",
        ],
        "notes__search_contents" => &["max_matches_per_file", "max_files"],
        "image_gen__imagegen" => &["num_last_images_to_include"],
        "web__run" => &[
            "search_query.items.properties.recency",
            "image_query.items.properties.recency",
            "open.items.properties.lineno",
            "click.items.properties.id",
            "screenshot.items.properties.pageno",
            "weather.items.properties.duration",
            "sports.items.properties.num_games",
        ],
        _ => &[],
    }
}

/// ref: internal/client/codex/tool-schema/tool_schema.go:165-211 @ d7914afd
fn normalize_raw_integer_field_types(parameters: &str, fields: &[&str]) -> Option<String> {
    let params = gjson::parse(parameters);
    let properties = params.get("properties");
    if fields.is_empty() || properties.kind() != Kind::Object {
        return None;
    }
    let mut output = parameters.as_bytes().to_vec();
    let mut changed = false;
    for field in fields {
        let property = properties.get(field);
        let type_value = property.get("type");
        let replacement = if type_value.kind() == Kind::String && type_value.str() == "number" {
            Some(br#""integer""#.to_vec())
        } else if type_value.kind() == Kind::Array {
            let mut seen = HashSet::new();
            let mut values = Vec::new();
            let mut has_number = false;
            type_value.each(|_, item| {
                let text = if item.kind() == Kind::String {
                    item.str()
                } else if item.kind() == Kind::Null {
                    ""
                } else {
                    item.json()
                };
                let text = if text == "number" {
                    has_number = true;
                    "integer"
                } else {
                    text
                };
                if seen.insert(text.to_owned()) {
                    values.push(text.to_owned());
                }
                true
            });
            has_number
                .then(|| serde_json::to_vec(&values).ok())
                .flatten()
        } else {
            None
        };
        if let Some(replacement) = replacement {
            let updated = set_raw_path(&output, &format!("properties.{field}.type"), &replacement);
            changed |= updated != output;
            output = updated;
        }
    }
    changed.then(|| String::from_utf8(output).ok()).flatten()
}

fn normalize_tool_list_json(tools_json: &str) -> Option<String> {
    let tools = gjson::parse(tools_json);
    if tools.kind() != Kind::Array {
        return None;
    }
    let raw = tools.json();
    let mut scan = 0;
    let mut splice_from = 0;
    let mut out = String::new();
    let mut changed = false;
    let mut failed = false;
    tools.each(|_, tool| {
        if failed {
            return false;
        }
        let tool_raw = tool.json();
        if tool_raw.is_empty() {
            failed = true;
            return false;
        }
        let Some(relative) = raw[scan..].find(tool_raw) else {
            failed = true;
            return false;
        };
        let start = scan + relative;
        scan = start + tool_raw.len();
        if let Some(updated) = normalize_tool_json(tool_raw) {
            out.push_str(&raw[splice_from..start]);
            out.push_str(&updated);
            splice_from = scan;
            changed = true;
        }
        true
    });
    if failed || !changed {
        return None;
    }
    out.push_str(&raw[splice_from..]);
    Some(out)
}

fn normalize_tool_json(tool_json: &str) -> Option<String> {
    let tool = gjson::parse(tool_json);
    let kind = {
        let kind_value = tool.get("type");
        kind_value.str().to_owned()
    };
    if kind == "namespace" {
        let nested = tool.get("tools");
        let updated = normalize_tool_list_json(nested.json())?;
        return replace_object_field(tool_json, "tools", &updated);
    }
    if kind != "function" && kind != "custom" {
        return None;
    }
    let mut parsed: Value = serde_json::from_str(tool_json).ok()?;
    let parameters = parsed.get_mut("parameters")?;
    if !parameters.is_object() {
        return None;
    }
    let mut changed = strip_unsupported_schema_patterns(parameters);
    changed |= normalize_top_level_const_unions(parameters);
    if !changed {
        return None;
    }
    serde_json::to_string(&parsed).ok()
}

fn normalize_top_level_const_unions(parameters: &mut Value) -> bool {
    let Some(properties) = parameters
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let mut changed = false;
    for property in properties.values_mut() {
        changed |= normalize_property_union(property);
    }
    changed
}

fn normalize_property_union(property: &mut Value) -> bool {
    let Some(object) = property.as_object() else {
        return false;
    };
    let has_one_of = object.contains_key("oneOf");
    let has_any_of = object.contains_key("anyOf");
    if has_one_of == has_any_of {
        return false;
    }
    let union_name = if has_one_of { "oneOf" } else { "anyOf" };
    let Some(branches) = object.get(union_name).and_then(Value::as_array) else {
        return false;
    };
    if branches.len() < CODEX_COMPLEX_UNION_BRANCH_THRESHOLD {
        return false;
    }
    let mut keys = Vec::with_capacity(branches.len());
    let mut consts = Vec::with_capacity(branches.len());
    let mut seen = HashSet::with_capacity(branches.len());
    for branch in branches {
        let Some((key, const_value)) = pure_const_branch(branch) else {
            return false;
        };
        if !seen.insert(key.clone()) {
            return false;
        }
        keys.push(key);
        consts.push(const_value);
    }
    let Some(object) = property.as_object_mut() else {
        return false;
    };
    if let Some(existing) = object.get("enum").cloned() {
        let Some(existing) = existing.as_array() else {
            return false;
        };
        let mut existing_keys = Vec::with_capacity(existing.len());
        for item in existing {
            let Some(key) = canonical_json_value_key(item) else {
                return false;
            };
            existing_keys.push(key);
        }
        if !equal_canonical_sets(&existing_keys, &keys) {
            return false;
        }
        object.remove(union_name);
        return true;
    }
    object.insert("enum".to_owned(), Value::Array(consts));
    object.remove(union_name);
    true
}

fn pure_const_branch(branch: &Value) -> Option<(String, Value)> {
    let object = branch.as_object()?;
    let const_value = object.get("const")?.clone();
    if object
        .keys()
        .any(|key| key != "const" && key != "description" && key != "title")
    {
        return None;
    }
    Some((canonical_json_value_key(&const_value)?, const_value))
}

fn canonical_json_value_key(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(format!("s:{text}")),
        Value::Number(number) => Some(canonical_number_key(&number.to_string())),
        Value::Bool(true) => Some("b:true".to_owned()),
        Value::Bool(false) => Some("b:false".to_owned()),
        Value::Null => Some("null".to_owned()),
        _ => None,
    }
}

fn canonical_number_key(raw: &str) -> String {
    let trimmed = raw.trim();
    match reduced_ratio(trimmed) {
        Some(ratio) => format!("n:{ratio}"),
        None => format!("n:{trimmed}"),
    }
}

fn reduced_ratio(raw: &str) -> Option<String> {
    if let Some((left, right)) = raw.split_once('/') {
        let numerator = parse_signed_int(left)?;
        let denominator = parse_signed_int(right)?;
        if denominator == 0 {
            return None;
        }
        return Some(format_ratio(numerator, denominator));
    }
    let (mantissa, exponent) = split_scientific(raw)?;
    let (mut numerator, scale) = parse_decimal_mantissa(mantissa)?;
    let mut denominator: i128 = 1;
    if exponent >= 0 {
        numerator = numerator.checked_mul(pow10(exponent as u32)?)?;
    } else {
        denominator = denominator.checked_mul(pow10((-exponent) as u32)?)?;
    }
    denominator = denominator.checked_mul(pow10(scale)?)?;
    Some(format_ratio(numerator, denominator))
}

fn split_scientific(raw: &str) -> Option<(&str, i32)> {
    let Some(index) = raw.find(['e', 'E']) else {
        return Some((raw, 0));
    };
    let exponent = raw[index + 1..].parse::<i32>().ok()?;
    Some((&raw[..index], exponent))
}

fn parse_decimal_mantissa(raw: &str) -> Option<(i128, u32)> {
    let (sign, digits) = split_sign(raw)?;
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let joined = format!("{whole}{fraction}");
    let magnitude = if joined.bytes().all(|byte| byte == b'0') {
        0
    } else {
        joined.parse::<i128>().ok()?
    };
    let numerator = if sign < 0 { -magnitude } else { magnitude };
    u32::try_from(fraction.len())
        .ok()
        .map(|scale| (numerator, scale))
}

fn parse_signed_int(raw: &str) -> Option<i128> {
    let (sign, digits) = split_sign(raw.trim())?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let magnitude = digits.parse::<i128>().ok()?;
    Some(if sign < 0 { -magnitude } else { magnitude })
}

fn split_sign(raw: &str) -> Option<(i32, &str)> {
    if let Some(rest) = raw.strip_prefix('+') {
        Some((1, rest))
    } else if let Some(rest) = raw.strip_prefix('-') {
        Some((-1, rest))
    } else {
        Some((1, raw))
    }
}

fn pow10(exponent: u32) -> Option<i128> {
    let mut value: i128 = 1;
    for _ in 0..exponent {
        value = value.checked_mul(10)?;
    }
    Some(value)
}

fn format_ratio(mut numerator: i128, mut denominator: i128) -> String {
    if denominator < 0 {
        numerator = -numerator;
        denominator = -denominator;
    }
    let divisor = gcd(numerator.unsigned_abs(), denominator as u128);
    numerator /= i128::try_from(divisor).unwrap_or(1);
    denominator /= i128::try_from(divisor).unwrap_or(1);
    format!("{numerator}/{denominator}")
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left
}

fn equal_canonical_sets(left: &[String], right: &[String]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let unique = left.iter().collect::<HashSet<_>>();
    unique.len() == left.len() && right.iter().all(|key| unique.contains(key))
}

fn replace_object_field(object_json: &str, field: &str, new_value: &str) -> Option<String> {
    let (start, end) = first_object_field_span(object_json, field)?;
    let mut out = String::with_capacity(object_json.len() - (end - start) + new_value.len());
    out.push_str(&object_json[..start]);
    out.push_str(new_value);
    out.push_str(&object_json[end..]);
    Some(out)
}

pub(crate) fn first_object_field_span(document: &str, name: &str) -> Option<(usize, usize)> {
    let bytes = document.as_bytes();
    let mut index = skip_ws(bytes, 0);
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    index += 1;
    loop {
        index = skip_ws(bytes, index);
        if bytes.get(index) == Some(&b'}') {
            return None;
        }
        let (key, next) = parse_json_string(bytes, index)?;
        index = skip_ws(bytes, next);
        if bytes.get(index) != Some(&b':') {
            return None;
        }
        index = skip_ws(bytes, index + 1);
        let start = index;
        index = skip_json_value(bytes, index)?;
        if key == name {
            return Some((start, index));
        }
        index = skip_ws(bytes, index);
        if bytes.get(index) == Some(&b',') {
            index += 1;
            continue;
        }
        return None;
    }
}

fn skip_ws(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    index
}

fn skip_json_value(bytes: &[u8], index: usize) -> Option<usize> {
    match bytes.get(index)? {
        b'"' => parse_json_string(bytes, index).map(|(_, next)| next),
        b'{' => skip_json_object(bytes, index),
        b'[' => skip_json_array(bytes, index),
        b't' => skip_literal(bytes, index, b"true"),
        b'f' => skip_literal(bytes, index, b"false"),
        b'n' => skip_literal(bytes, index, b"null"),
        b'-' | b'0'..=b'9' => skip_number(bytes, index),
        _ => None,
    }
}

fn skip_json_array(bytes: &[u8], mut index: usize) -> Option<usize> {
    index += 1;
    loop {
        index = skip_ws(bytes, index);
        if bytes.get(index) == Some(&b']') {
            return Some(index + 1);
        }
        index = skip_json_value(bytes, index)?;
        index = skip_ws(bytes, index);
        if bytes.get(index) == Some(&b',') {
            index += 1;
        }
    }
}

fn skip_json_object(bytes: &[u8], mut index: usize) -> Option<usize> {
    index += 1;
    loop {
        index = skip_ws(bytes, index);
        if bytes.get(index) == Some(&b'}') {
            return Some(index + 1);
        }
        let (_, next) = parse_json_string(bytes, index)?;
        index = skip_ws(bytes, next);
        if bytes.get(index) != Some(&b':') {
            return None;
        }
        index = skip_json_value(bytes, skip_ws(bytes, index + 1))?;
        index = skip_ws(bytes, index);
        if bytes.get(index) == Some(&b',') {
            index += 1;
        }
    }
}

fn skip_literal(bytes: &[u8], index: usize, literal: &[u8]) -> Option<usize> {
    bytes
        .get(index..index + literal.len())
        .filter(|slice| *slice == literal)
        .map(|_| index + literal.len())
}

fn skip_number(bytes: &[u8], mut index: usize) -> Option<usize> {
    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    if bytes.get(index) == Some(&b'0') {
        index += 1;
    } else if bytes.get(index).is_some_and(u8::is_ascii_digit) {
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    } else {
        return None;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return None;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return None;
        }
    }
    Some(index)
}

fn parse_json_string(bytes: &[u8], index: usize) -> Option<(String, usize)> {
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    let mut index = index + 1;
    let mut decoded = String::new();
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return Some((decoded, index + 1)),
            b'\\' => {
                index += 1;
                let escaped = *bytes.get(index)?;
                match escaped {
                    b'"' | b'\\' | b'/' => decoded.push(escaped as char),
                    b'b' => decoded.push('\u{0008}'),
                    b'f' => decoded.push('\u{000c}'),
                    b'n' => decoded.push('\n'),
                    b'r' => decoded.push('\r'),
                    b't' => decoded.push('\t'),
                    b'u' => {
                        let hex = std::str::from_utf8(bytes.get(index + 1..index + 5)?).ok()?;
                        let code = u32::from_str_radix(hex, 16).ok()?;
                        decoded.push(char::from_u32(code)?);
                        index += 4;
                    }
                    _ => return None,
                }
                index += 1;
            }
            byte if byte < 0x80 => {
                decoded.push(byte as char);
                index += 1;
            }
            _ => {
                let rest = std::str::from_utf8(&bytes[index..]).ok()?;
                let character = rest.chars().next()?;
                decoded.push(character);
                index += character.len_utf8();
            }
        }
    }
    None
}
