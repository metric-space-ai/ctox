// ref: internal/util/gemini_schema.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashSet;

use serde_json::{Map, Value};

const PLACEHOLDER_REASON_DESCRIPTION: &str = "Brief explanation of why you are calling this tool";
const UNSUPPORTED_SCHEMA_KEYS: &[&str] = &[
    "minLength",
    "maxLength",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "pattern",
    "minItems",
    "maxItems",
    "uniqueItems",
    "format",
    "default",
    "examples",
    "$schema",
    "$defs",
    "definitions",
    "const",
    "$ref",
    "$id",
    "additionalProperties",
    "propertyNames",
    "patternProperties",
    "$comment",
    "enumDescriptions",
    "enumTitles",
    "prefill",
    "deprecated",
];
const CONSTRAINT_KEYS: &[&str] = &[
    "minLength",
    "maxLength",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "pattern",
    "minItems",
    "maxItems",
    "uniqueItems",
    "format",
    "default",
    "examples",
];
const SCHEMA_NAME_MAP_KEYS: &[&str] = &[
    "properties",
    "patternProperties",
    "dependentSchemas",
    "$defs",
    "definitions",
];

#[derive(Clone, Copy, Default)]
struct CleanOptions {
    add_placeholder: bool,
    remove_gemini_metadata: bool,
    flatten_unions: bool,
    force_enum_string_type: bool,
}

/// Cleans exactly one JSON schema for Gemini tool calling.
///
/// Callers must pass the schema node itself, never a request document. Schema
/// keywords are ordinary argument names outside this boundary and recursively
/// cleaning a whole request would corrupt function-call history.
#[must_use]
pub fn clean_json_schema_for_gemini(schema: &Value) -> Value {
    clean_json_schema(
        schema,
        CleanOptions {
            remove_gemini_metadata: true,
            flatten_unions: true,
            force_enum_string_type: true,
            ..CleanOptions::default()
        },
    )
}

/// Cleans an Antigravity tool schema and adds the placeholders required by
/// Claude's `VALIDATED` function-calling mode.
#[must_use]
pub fn clean_json_schema_for_antigravity(schema: &Value) -> Value {
    clean_json_schema(
        schema,
        CleanOptions {
            add_placeholder: true,
            flatten_unions: true,
            force_enum_string_type: true,
            ..CleanOptions::default()
        },
    )
}

/// Cleans an Antigravity structured-response schema without applying the
/// tool-only union, enum-type, or placeholder rewrites.
#[must_use]
pub fn clean_json_schema_for_antigravity_response(schema: &Value) -> Value {
    clean_json_schema(schema, CleanOptions::default())
}

fn clean_json_schema(schema: &Value, options: CleanOptions) -> Value {
    let mut cleaned = schema.clone();

    // Keep the phase order aligned with the pinned Go source. Several phases
    // intentionally consume information which a later cleanup removes.
    convert_refs_to_hints(&mut cleaned);
    walk_maps_mut(&mut cleaned, false, &mut |object, _| {
        convert_const_to_enum(object)
    });
    walk_maps_mut(&mut cleaned, false, &mut |object, _| {
        convert_enum_values_to_strings(object, options.force_enum_string_type)
    });
    walk_maps_mut(&mut cleaned, false, &mut |object, _| add_enum_hint(object));
    walk_maps_mut(&mut cleaned, false, &mut |object, _| {
        add_additional_properties_hint(object)
    });
    walk_maps_mut(&mut cleaned, false, &mut |object, is_properties_map| {
        move_constraints_to_description(object, is_properties_map)
    });

    merge_all_of(&mut cleaned);
    if options.flatten_unions {
        flatten_union(&mut cleaned, "anyOf");
        flatten_union(&mut cleaned, "oneOf");
    }
    flatten_type_arrays(&mut cleaned, false);

    walk_maps_mut(&mut cleaned, false, &mut |object, is_properties_map| {
        remove_unsupported_keywords(object, is_properties_map)
    });
    if options.remove_gemini_metadata {
        walk_maps_mut(&mut cleaned, false, &mut |object, is_properties_map| {
            if !is_properties_map {
                object.remove("nullable");
                object.remove("title");
            }
        });
        remove_placeholder_fields(&mut cleaned);
    }
    cleanup_required_fields(&mut cleaned);
    // ref: internal/util/gemini_schema.go:162-186 @ a2976eb8
    walk_maps_mut(&mut cleaned, false, &mut |object, is_name_map| {
        if !is_name_map && object.get("properties").is_some_and(Value::is_object) {
            object.insert("type".to_owned(), Value::String("object".to_owned()));
        }
    });
    if options.add_placeholder {
        add_empty_schema_placeholder(&mut cleaned, true);
    }

    cleaned
}

fn walk_maps_mut(
    value: &mut Value,
    is_name_map: bool,
    visit: &mut impl FnMut(&mut Map<String, Value>, bool),
) {
    match value {
        Value::Array(values) => {
            for value in values {
                walk_maps_mut(value, false, visit);
            }
        }
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                // A schema keyword opens a map of author-chosen names only
                // when its parent is itself a schema node. If the current
                // object already is a name map, a child named `properties`
                // is the schema of a property with that name and must be
                // cleaned normally. This is the recursive equivalent of the
                // candidate upstream's trailing-name-map parity check.
                let child_is_name_map = !is_name_map && is_schema_name_map_key(key);
                walk_maps_mut(child, child_is_name_map, visit);
            }
            visit(object, is_name_map);
        }
        _ => {}
    }
}

fn is_schema_name_map_key(key: &str) -> bool {
    SCHEMA_NAME_MAP_KEYS.contains(&key)
}

fn convert_refs_to_hints(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                convert_refs_to_hints(value);
            }
        }
        Value::Object(object) => {
            for child in object.values_mut() {
                convert_refs_to_hints(child);
            }
            let Some(reference) = object.get("$ref").and_then(Value::as_str) else {
                return;
            };
            let name = reference.rsplit('/').next().unwrap_or(reference);
            let hint = format!("See: {name}");
            let description = object
                .get("description")
                .and_then(Value::as_str)
                .map(|existing| format!("{existing} ({hint})"))
                .unwrap_or(hint);
            *value = serde_json::json!({"type":"object","description":description});
        }
        _ => {}
    }
}

fn convert_const_to_enum(object: &mut Map<String, Value>) {
    if !object.contains_key("enum") {
        if let Some(constant) = object.get("const").cloned() {
            object.insert("enum".to_owned(), Value::Array(vec![constant]));
        }
    }
}

fn convert_enum_values_to_strings(object: &mut Map<String, Value>, force_string_type: bool) {
    let Some(values) = object.get_mut("enum").and_then(Value::as_array_mut) else {
        return;
    };
    for value in values {
        *value = Value::String(value_string(value));
    }
    if force_string_type {
        object.insert("type".to_owned(), Value::String("string".to_owned()));
    }
}

fn add_enum_hint(object: &mut Map<String, Value>) {
    let Some(values) = object.get("enum").and_then(Value::as_array) else {
        return;
    };
    if !(2..=10).contains(&values.len()) {
        return;
    }
    let allowed = values
        .iter()
        .map(value_string)
        .collect::<Vec<_>>()
        .join(", ");
    append_hint(object, &format!("Allowed: {allowed}"));
}

fn add_additional_properties_hint(object: &mut Map<String, Value>) {
    if object.get("additionalProperties") == Some(&Value::Bool(false)) {
        append_hint(object, "No extra properties allowed");
    }
}

fn move_constraints_to_description(object: &mut Map<String, Value>, is_properties_map: bool) {
    if is_properties_map {
        return;
    }
    for key in CONSTRAINT_KEYS {
        let Some(value) = object
            .get(*key)
            .filter(|value| !value.is_array() && !value.is_object())
        else {
            continue;
        };
        append_hint(object, &format!("{key}: {}", value_string(value)));
    }
}

fn merge_all_of(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                merge_all_of(value);
            }
        }
        Value::Object(object) => {
            for child in object.values_mut() {
                merge_all_of(child);
            }
            let Some(items) = object
                .remove("allOf")
                .and_then(|value| value.as_array().cloned())
            else {
                return;
            };

            let mut merged_properties = Map::new();
            let mut merged_required = object
                .get("required")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for item in items {
                if let Some(properties) = item.get("properties").and_then(Value::as_object) {
                    for (name, property) in properties {
                        merged_properties.insert(name.clone(), property.clone());
                    }
                }
                if let Some(required) = item.get("required").and_then(Value::as_array) {
                    for name in required {
                        if !merged_required.contains(name) {
                            merged_required.push(name.clone());
                        }
                    }
                }
            }
            if !merged_properties.is_empty() {
                let properties = object
                    .entry("properties".to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(properties) = properties.as_object_mut() {
                    properties.extend(merged_properties);
                }
            }
            if !merged_required.is_empty() {
                object.insert("required".to_owned(), Value::Array(merged_required));
            }
        }
        _ => {}
    }
}

fn flatten_union(value: &mut Value, key: &str) {
    match value {
        Value::Array(values) => {
            for value in values {
                flatten_union(value, key);
            }
        }
        Value::Object(object) => {
            for child in object.values_mut() {
                flatten_union(child, key);
            }
            let Some(items) = object.get(key).and_then(Value::as_array) else {
                return;
            };
            if items.is_empty() {
                return;
            }

            let types = items.iter().map(schema_type).collect::<Vec<_>>();
            let mut best_index = 0;
            let mut best_score = -1_i8;
            for (index, item) in items.iter().enumerate() {
                let score = schema_score(item);
                if score > best_score {
                    best_index = index;
                    best_score = score;
                }
            }
            let mut selected = items[best_index].clone();
            let parent_description = object
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let Some(selected_object) = selected.as_object_mut() else {
                *value = selected;
                return;
            };
            if let Some(parent) = parent_description {
                merge_parent_description(selected_object, &parent);
            }
            if types.len() > 1 {
                append_hint(selected_object, &format!("Accepts: {}", types.join(" | ")));
            }
            *value = selected;
        }
        _ => {}
    }
}

fn flatten_type_arrays(value: &mut Value, direct_property: bool) {
    match value {
        Value::Array(values) => {
            for value in values {
                flatten_type_arrays(value, false);
            }
        }
        Value::Object(object) => {
            let nullable_properties = object
                .get("properties")
                .and_then(Value::as_object)
                .map(|properties| {
                    properties
                        .iter()
                        .filter(|(_, property)| type_array_contains_null(property))
                        .map(|(name, _)| name.clone())
                        .collect::<HashSet<_>>()
                })
                .unwrap_or_default();

            for (key, child) in object.iter_mut() {
                if key == "properties" {
                    if let Some(properties) = child.as_object_mut() {
                        for property in properties.values_mut() {
                            flatten_type_arrays(property, true);
                        }
                    }
                } else {
                    flatten_type_arrays(child, false);
                }
            }

            if let Some(types) = object.get("type").and_then(Value::as_array) {
                if !types.is_empty() {
                    let non_null = types
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|kind| *kind != "null" && !kind.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    let nullable = types.iter().any(|kind| kind.as_str() == Some("null"));
                    object.insert(
                        "type".to_owned(),
                        Value::String(
                            if object.contains_key("items")
                                && non_null.iter().any(|kind| kind == "array")
                            {
                                "array".to_owned()
                            } else if object.contains_key("properties")
                                && non_null.iter().any(|kind| kind == "object")
                            {
                                "object".to_owned()
                            } else {
                                non_null
                                    .first()
                                    .cloned()
                                    .unwrap_or_else(|| "string".to_owned())
                            },
                        ),
                    );
                    if non_null.len() > 1 {
                        append_hint(object, &format!("Accepts: {}", non_null.join(" | ")));
                    }
                    if nullable && direct_property {
                        append_hint(object, "(nullable)");
                    }
                }
            }

            if !nullable_properties.is_empty() {
                if let Some(required) = object.get_mut("required").and_then(Value::as_array_mut) {
                    required.retain(|name| {
                        name.as_str()
                            .is_none_or(|name| !nullable_properties.contains(name))
                    });
                    if required.is_empty() {
                        object.remove("required");
                    }
                }
            }
        }
        _ => {}
    }
}

fn type_array_contains_null(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|types| types.iter().any(|kind| kind.as_str() == Some("null")))
}

fn remove_unsupported_keywords(object: &mut Map<String, Value>, is_properties_map: bool) {
    if is_properties_map {
        return;
    }
    for key in UNSUPPORTED_SCHEMA_KEYS {
        object.remove(*key);
    }
    object.retain(|key, _| !key.starts_with("x-"));
}

fn remove_placeholder_fields(value: &mut Value) {
    walk_maps_mut(value, false, &mut |object, is_properties_map| {
        if is_properties_map {
            return;
        }
        let remove_reason = object
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|properties| {
                properties.len() == 1
                    && properties
                        .get("reason")
                        .and_then(|reason| reason.get("description"))
                        .and_then(Value::as_str)
                        == Some(PLACEHOLDER_REASON_DESCRIPTION)
            });
        if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
            properties.remove("_");
            if remove_reason {
                properties.remove("reason");
            }
        }
        if let Some(required) = object.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|name| {
                name.as_str() != Some("_") && (!remove_reason || name.as_str() != Some("reason"))
            });
            if required.is_empty() {
                object.remove("required");
            }
        }
    });
}

fn cleanup_required_fields(value: &mut Value) {
    walk_maps_mut(value, false, &mut |object, is_properties_map| {
        if is_properties_map {
            return;
        }
        let Some(valid) = object
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| properties.keys().cloned().collect::<HashSet<_>>())
        else {
            return;
        };
        let Some(required) = object.get_mut("required").and_then(Value::as_array_mut) else {
            return;
        };
        let original_len = required.len();
        required.retain(|name| name.as_str().is_some_and(|name| valid.contains(name)));
        if original_len != required.len() && required.is_empty() {
            object.remove("required");
        }
    });
}

fn add_empty_schema_placeholder(value: &mut Value, is_root: bool) {
    match value {
        Value::Array(values) => {
            for value in values {
                add_empty_schema_placeholder(value, false);
            }
        }
        Value::Object(object) => {
            for child in object.values_mut() {
                add_empty_schema_placeholder(child, false);
            }
            if object.get("type").and_then(Value::as_str) != Some("object") {
                return;
            }
            let properties_missing_or_empty = object
                .get("properties")
                .and_then(Value::as_object)
                .is_none_or(Map::is_empty);
            if properties_missing_or_empty {
                object.insert(
                    "properties".to_owned(),
                    serde_json::json!({
                        "reason": {
                            "type": "string",
                            "description": PLACEHOLDER_REASON_DESCRIPTION
                        }
                    }),
                );
                object.insert(
                    "required".to_owned(),
                    Value::Array(vec![Value::String("reason".to_owned())]),
                );
                return;
            }
            let has_required = object
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| !required.is_empty());
            if !is_root && !has_required {
                object
                    .get_mut("properties")
                    .and_then(Value::as_object_mut)
                    .expect("object properties retained")
                    .entry("_".to_owned())
                    .or_insert_with(|| serde_json::json!({"type":"boolean"}));
                object.insert(
                    "required".to_owned(),
                    Value::Array(vec![Value::String("_".to_owned())]),
                );
            }
        }
        _ => {}
    }
}

fn schema_score(value: &Value) -> i8 {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if kind == "object" || value.get("properties").is_some() {
        3
    } else if kind == "array" || value.get("items").is_some() {
        2
    } else if !kind.is_empty() && kind != "null" {
        1
    } else {
        0
    }
}

fn schema_type(value: &Value) -> String {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if kind == "object" || value.get("properties").is_some() {
        if kind.is_empty() {
            "object".to_owned()
        } else {
            kind.to_owned()
        }
    } else if kind == "array" || value.get("items").is_some() {
        if kind.is_empty() {
            "array".to_owned()
        } else {
            kind.to_owned()
        }
    } else if kind.is_empty() {
        "null".to_owned()
    } else {
        kind.to_owned()
    }
}

fn merge_parent_description(object: &mut Map<String, Value>, parent: &str) {
    match object.get("description").and_then(Value::as_str) {
        None | Some("") => {
            object.insert("description".to_owned(), Value::String(parent.to_owned()));
        }
        Some(child) if child != parent => {
            object.insert(
                "description".to_owned(),
                Value::String(format!("{parent} ({child})")),
            );
        }
        Some(_) => {}
    }
}

fn append_hint(object: &mut Map<String, Value>, hint: &str) {
    let existing = object
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if existing == hint
        || existing.starts_with(&format!("{hint} ("))
        || existing.contains(&format!("({hint})"))
    {
        return;
    }
    let merged = if existing.is_empty() {
        hint.to_owned()
    } else {
        format!("{existing} ({hint})")
    };
    object.insert("description".to_owned(), Value::String(merged));
}

fn value_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}
