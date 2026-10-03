// ref: internal/client/codex/optimize-multi-agent-v2/optimize_multi_agent_v2.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use crate::internal::translator::common::{delete_raw_path, set_raw_path};
use serde::Deserialize;
use serde_json::{Map, Value};

#[path = "orphan_delegation.rs"]
mod orphan_delegation;
pub use orphan_delegation::rewrite_orphan_delegation_input;

#[cfg(test)]
#[path = "input_compat_test.rs"]
mod input_compat_test;

pub const CODEX_SPAWN_AGENT_DESCRIPTION_MARKER: &str = "Spawns an agent";
pub const CODEX_SPAWN_AGENT_MODELS_HEADING: &str =
    "Available model overrides (optional; inherited parent model is preferred):";
pub const CODEX_COLLABORATION_NAMESPACE: &str = "collaboration";
pub const CODEX_OPTIMIZED_COLLABORATION_NAMESPACE: &str = "collaboration-optimize";
pub const CODEX_OPTIMIZED_COLLABORATION_NAME_PREFIX: &str = "collaboration-optimize__";
const COLLABORATION_MESSAGE_TOOLS: [&str; 3] = ["spawn_agent", "send_message", "followup_task"];

pub type ModelMap = Map<String, Value>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpawnAgentModel {
    pub id: String,
    pub description: String,
    pub reasoning_efforts: Vec<String>,
    pub default_reasoning_effort: String,
    pub service_tiers: Vec<String>,
    pub priority: i64,
    pub display_name: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpawnAgentModelMetadata {
    pub description: String,
    pub thinking_levels: Vec<String>,
}

pub trait SpawnAgentModelMetadataSource {
    fn lookup(&self, model_id: &str) -> Option<SpawnAgentModelMetadata>;
}

impl<F> SpawnAgentModelMetadataSource for F
where
    F: Fn(&str) -> Option<SpawnAgentModelMetadata>,
{
    fn lookup(&self, model_id: &str) -> Option<SpawnAgentModelMetadata> {
        self(model_id)
    }
}

#[derive(Clone, Debug, Default)]
pub struct MultiAgentV2Context {
    pub enabled: bool,
    pub user_agent: String,
    pub available_models: Vec<ModelMap>,
    pub catalog_json: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptimizeResult {
    pub payload: Vec<u8>,
    pub namespace_optimized: bool,
}

#[must_use]
pub fn is_codex_multi_agent_client(user_agent: &str) -> bool {
    let user_agent = user_agent.trim();
    user_agent.starts_with("Codex Desktop/") || user_agent.starts_with("codex-tui/")
}

#[must_use]
pub fn enabled(context: &MultiAgentV2Context) -> bool {
    context.enabled && is_codex_multi_agent_client(&context.user_agent)
}

#[must_use]
pub fn optimize_request(
    context: &MultiAgentV2Context,
    payload: &[u8],
    metadata: &dyn SpawnAgentModelMetadataSource,
) -> OptimizeResult {
    if !enabled(context) {
        return unchanged(payload);
    }
    let Ok(mut value) = serde_json::from_slice::<Value>(payload) else {
        return unchanged(payload);
    };
    rewrite_agent_message_content(&mut value);
    remove_collaboration_message_encryption(&mut value);
    if has_optimized_collaboration_conflict(&value) {
        return encoded(value, false, payload);
    }
    let models =
        spawn_agent_models_from_sources(&context.available_models, &context.catalog_json, metadata);
    rewrite_spawn_agent_tools(&mut value, &models);
    let optimized = optimize_collaboration_namespaces(&mut value);
    encoded(value, optimized, payload)
}

#[must_use]
pub fn rewrite_spawn_agent_description(
    context: &MultiAgentV2Context,
    payload: &[u8],
    metadata: &dyn SpawnAgentModelMetadataSource,
) -> Vec<u8> {
    optimize_request(context, payload, metadata).payload
}

#[must_use]
pub fn rewrite_multi_agent_input(context: &MultiAgentV2Context, payload: &[u8]) -> Vec<u8> {
    rewrite_multi_agent_input_with_compat(context, payload, false)
}

/// ref: internal/client/codex/optimize-multi-agent-v2/optimize_multi_agent_v2.go:71-78,788-833
/// Compatibility mode forces portable messages independently of client detection
/// and strips internal metadata from every input item.
#[must_use]
pub fn rewrite_multi_agent_input_with_compat(
    context: &MultiAgentV2Context,
    payload: &[u8],
    is_compat: bool,
) -> Vec<u8> {
    if !is_compat && !enabled(context) {
        return payload.to_vec();
    }
    let input = crate::internal::util::get_gjson_bytes_no_copy(payload, "input");
    if input.kind() != gjson::Kind::Array {
        return payload.to_vec();
    }
    let mut updated = payload.to_vec();
    let mut index = 0;
    input.each(|_, item| {
        let path = format!("input.{index}");
        if item.get("type").str().trim() == "agent_message" {
            let content = item.get("content");
            if content.kind() == gjson::Kind::Array {
                let mut part_index = 0;
                content.each(|_, part| {
                    let encrypted = part.get("encrypted_content");
                    if part.get("type").str().trim() == "encrypted_content"
                        && encrypted.kind() == gjson::Kind::String
                    {
                        let part_path = format!("{path}.content.{part_index}");
                        updated = set_raw_path(
                            &updated,
                            &format!("{part_path}.type"),
                            br#""input_text""#,
                        );
                        let text = serde_json::to_vec(encrypted.str()).expect("string JSON");
                        updated = set_raw_path(&updated, &format!("{part_path}.text"), &text);
                        updated =
                            delete_raw_path(&updated, &format!("{part_path}.encrypted_content"));
                    }
                    part_index += 1;
                    true
                });
            }
            updated = set_raw_path(&updated, &format!("{path}.role"), br#""user""#);
            updated = set_raw_path(&updated, &format!("{path}.type"), br#""message""#);
        }
        if is_compat {
            for key in [
                "author",
                "recipient",
                "internal_chat_message_metadata_passthrough",
            ] {
                if item.get(key).exists() {
                    updated = delete_raw_path(&updated, &format!("{path}.{key}"));
                }
            }
        }
        index += 1;
        true
    });
    updated
}

#[must_use]
pub fn restore_response(payload: &[u8], optimized: bool) -> Vec<u8> {
    if !optimized || payload.is_empty() {
        return payload.to_vec();
    }
    let Ok(mut value) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    if !restore_collaboration_value(&mut value) {
        return payload.to_vec();
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| payload.to_vec())
}

fn unchanged(payload: &[u8]) -> OptimizeResult {
    OptimizeResult {
        payload: payload.to_vec(),
        namespace_optimized: false,
    }
}

fn encoded(value: Value, optimized: bool, original: &[u8]) -> OptimizeResult {
    OptimizeResult {
        payload: serde_json::to_vec(&value).unwrap_or_else(|_| original.to_vec()),
        namespace_optimized: optimized,
    }
}

#[derive(Deserialize)]
struct Catalog {
    models: Vec<ModelMap>,
}

#[must_use]
pub fn decode_home_available_models(raw: &[u8]) -> Vec<ModelMap> {
    let Ok(sections) = serde_json::from_slice::<BTreeMap<String, Vec<ModelMap>>>(raw) else {
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    let mut models = Vec::new();
    for model in sections.into_values().flatten() {
        let mut model_id = map_string(&model, "id");
        if model_id.is_empty() {
            model_id = map_string(&model, "name")
                .strip_prefix("models/")
                .unwrap_or_default()
                .to_owned();
        }
        if model_id.is_empty() || !seen.insert(model_id.clone()) {
            continue;
        }
        let mut display_name = map_string(&model, "display_name");
        if display_name.is_empty() {
            display_name = map_string(&model, "displayName");
        }
        let mut entry = Map::from_iter([("id".to_owned(), Value::String(model_id))]);
        if !display_name.is_empty() {
            entry.insert(
                "display_name".to_owned(),
                Value::String(display_name.clone()),
            );
            entry.insert("description".to_owned(), Value::String(display_name));
        }
        models.push(entry);
    }
    models.sort_by_key(|model| map_string(model, "id"));
    models
}

#[must_use]
pub fn spawn_agent_models_from_sources(
    available_models: &[ModelMap],
    catalog_json: &[u8],
    metadata: &dyn SpawnAgentModelMetadataSource,
) -> Vec<SpawnAgentModel> {
    let Ok(catalog) = serde_json::from_slice::<Catalog>(catalog_json) else {
        return Vec::new();
    };
    let templates: BTreeMap<_, _> = catalog
        .models
        .into_iter()
        .filter_map(|model| {
            let slug = map_string(&model, "slug");
            (!slug.is_empty()).then_some((slug, model))
        })
        .collect();
    let Some(default_template) = templates.get("gpt-5.5") else {
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    let mut official = Vec::new();
    let mut synthesized = Vec::new();
    for available in available_models {
        let id = map_string(available, "id");
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        if let Some(template) = templates.get(&id) {
            official.push(spawn_agent_model_from_metadata(&id, template));
            continue;
        }
        let mut profile = spawn_agent_model_from_metadata(&id, default_template);
        profile.id = id.clone();
        profile.description = map_string(available, "description");
        profile.display_name = map_string(available, "display_name");
        if profile.display_name.is_empty() {
            profile.display_name.clone_from(&id);
        }
        if let Some(metadata) = metadata.lookup(&id) {
            if !metadata.description.trim().is_empty() {
                profile.description = metadata.description.trim().to_owned();
            }
            apply_spawn_agent_thinking(&mut profile, &metadata.thinking_levels);
        }
        if profile.description.is_empty() {
            profile.description = id;
        }
        profile.service_tiers.clear();
        synthesized.push(profile);
    }
    official.sort_by(|left, right| (left.priority, &left.id).cmp(&(right.priority, &right.id)));
    synthesized.sort_by(|left, right| {
        (left.display_name.to_ascii_lowercase(), &left.id)
            .cmp(&(right.display_name.to_ascii_lowercase(), &right.id))
    });
    official.extend(synthesized);
    official
}

fn spawn_agent_model_from_metadata(model_id: &str, metadata: &ModelMap) -> SpawnAgentModel {
    let (reasoning_efforts, default_reasoning_effort) = reasoning_metadata(metadata);
    SpawnAgentModel {
        id: model_id.to_owned(),
        description: map_string(metadata, "description"),
        display_name: map_string(metadata, "display_name"),
        priority: map_integer(metadata, "priority"),
        reasoning_efforts,
        default_reasoning_effort,
        service_tiers: service_tier_ids(metadata),
    }
}

fn apply_spawn_agent_thinking(profile: &mut SpawnAgentModel, raw_levels: &[String]) {
    let efforts: Vec<_> = raw_levels
        .iter()
        .filter_map(|effort| normalize_reasoning_effort(effort))
        .collect();
    if efforts.is_empty() {
        return;
    }
    profile.default_reasoning_effort = efforts
        .iter()
        .find(|effort| effort.as_str() == "medium")
        .or_else(|| efforts.iter().find(|effort| effort.as_str() != "none"))
        .unwrap_or(&efforts[0])
        .clone();
    profile.reasoning_efforts = efforts;
}

fn reasoning_metadata(metadata: &ModelMap) -> (Vec<String>, String) {
    let efforts: Vec<_> = metadata
        .get("supported_reasoning_levels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|level| normalize_reasoning_effort(&map_string(level, "effort")))
        .collect();
    if efforts.is_empty() {
        return (Vec::new(), String::new());
    }
    let default = normalize_reasoning_effort(&map_string(metadata, "default_reasoning_level"))
        .filter(|default| efforts.contains(default))
        .unwrap_or_else(|| efforts[0].clone());
    (efforts, default)
}

fn normalize_reasoning_effort(effort: &str) -> Option<String> {
    let effort = effort.trim().to_ascii_lowercase();
    matches!(
        effort.as_str(),
        "none" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
    )
    .then_some(effort)
}

fn service_tier_ids(metadata: &ModelMap) -> Vec<String> {
    let mut seen = BTreeSet::new();
    metadata
        .get("service_tiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .map(|tier| map_string(tier, "id"))
        .filter(|id| !id.is_empty() && seen.insert(id.clone()))
        .collect()
}

fn rewrite_spawn_agent_tools(value: &mut Value, models: &[SpawnAgentModel]) {
    let model_list = format_spawn_agent_models(models);
    visit_tool_arrays(value, &mut |tool| {
        if tool.get("type").and_then(Value::as_str) != Some("function")
            || tool.get("name").and_then(Value::as_str) != Some("spawn_agent")
        {
            return;
        }
        if !model_list.is_empty() {
            if let Some(description) = tool
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                let rewritten = replace_spawn_agent_models(&description, &model_list);
                tool.insert("description".to_owned(), Value::String(rewritten));
            }
        }
        remove_encrypted(tool);
    });
}

fn remove_collaboration_message_encryption(value: &mut Value) {
    visit_tool_arrays(value, &mut |tool| {
        let is_target = tool.get("type").and_then(Value::as_str) == Some("function")
            && tool
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| COLLABORATION_MESSAGE_TOOLS.contains(&name));
        if is_target {
            remove_encrypted(tool);
        }
    });
}

fn remove_encrypted(tool: &mut ModelMap) {
    if let Some(message) = tool
        .get_mut("parameters")
        .and_then(Value::as_object_mut)
        .and_then(|parameters| parameters.get_mut("properties"))
        .and_then(Value::as_object_mut)
        .and_then(|properties| properties.get_mut("message"))
        .and_then(Value::as_object_mut)
    {
        message.remove("encrypted");
    }
}

fn visit_tool_arrays(value: &mut Value, visitor: &mut impl FnMut(&mut ModelMap)) {
    if let Some(tools) = value.get_mut("tools").and_then(Value::as_array_mut) {
        visit_tools(tools, visitor);
    }
    if let Some(input) = value.get_mut("input").and_then(Value::as_array_mut) {
        for item in input {
            if item.get("type").and_then(Value::as_str) == Some("additional_tools") {
                if let Some(tools) = item.get_mut("tools").and_then(Value::as_array_mut) {
                    visit_tools(tools, visitor);
                }
            }
        }
    }
}

fn visit_tools(tools: &mut [Value], visitor: &mut impl FnMut(&mut ModelMap)) {
    for tool in tools {
        let Some(tool) = tool.as_object_mut() else {
            continue;
        };
        visitor(tool);
        if tool.get("type").and_then(Value::as_str) == Some("namespace") {
            if let Some(children) = tool.get_mut("tools").and_then(Value::as_array_mut) {
                visit_tools(children, visitor);
            }
        }
    }
}

fn has_optimized_collaboration_conflict(value: &Value) -> bool {
    tool_arrays(value).into_iter().any(tools_have_conflict)
}

fn tool_arrays(value: &Value) -> Vec<&[Value]> {
    let mut arrays = Vec::new();
    if let Some(tools) = value.get("tools").and_then(Value::as_array) {
        arrays.push(tools.as_slice());
    }
    if let Some(input) = value.get("input").and_then(Value::as_array) {
        arrays.extend(input.iter().filter_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("additional_tools"))
                .then(|| {
                    item.get("tools")
                        .and_then(Value::as_array)
                        .map(Vec::as_slice)
                })
                .flatten()
        }));
    }
    arrays
}

fn tools_have_conflict(tools: &[Value]) -> bool {
    tools.iter().any(|tool| {
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        name == CODEX_OPTIMIZED_COLLABORATION_NAMESPACE
            || name.starts_with(CODEX_OPTIMIZED_COLLABORATION_NAME_PREFIX)
            || (tool.get("type").and_then(Value::as_str) == Some("namespace")
                && tool
                    .get("tools")
                    .and_then(Value::as_array)
                    .is_some_and(|children| tools_have_conflict(children)))
    })
}

fn optimize_collaboration_namespaces(value: &mut Value) -> bool {
    let mut optimized = false;
    visit_tool_arrays(value, &mut |tool| {
        if tool.get("type").and_then(Value::as_str) == Some("namespace")
            && tool.get("name").and_then(Value::as_str) == Some(CODEX_COLLABORATION_NAMESPACE)
            && contains_spawn_agent(tool)
        {
            tool.insert(
                "name".to_owned(),
                Value::String(CODEX_OPTIMIZED_COLLABORATION_NAMESPACE.to_owned()),
            );
            optimized = true;
        }
    });
    optimized
}

fn contains_spawn_agent(tool: &ModelMap) -> bool {
    tool.get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| {
            tools.iter().any(|child| {
                child.get("type").and_then(Value::as_str) == Some("function")
                    && child.get("name").and_then(Value::as_str) == Some("spawn_agent")
            })
        })
}

fn rewrite_agent_message_content(value: &mut Value) {
    let Some(input) = value.get_mut("input").and_then(Value::as_array_mut) else {
        return;
    };
    for item in input {
        if item.get("type").and_then(Value::as_str) != Some("agent_message") {
            continue;
        }
        let Some(content) = item.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        for part in content {
            let Some(part) = part.as_object_mut() else {
                continue;
            };
            if part.get("type").and_then(Value::as_str) == Some("encrypted_content") {
                if let Some(encrypted) = part
                    .remove("encrypted_content")
                    .and_then(|v| v.as_str().map(str::to_owned))
                {
                    part.insert("type".to_owned(), Value::String("input_text".to_owned()));
                    part.insert("text".to_owned(), Value::String(encrypted));
                }
            }
        }
    }
}

fn restore_collaboration_value(value: &mut Value) -> bool {
    let mut changed = false;
    match value {
        Value::Array(values) => {
            for value in values {
                changed |= restore_collaboration_value(value);
            }
        }
        Value::Object(object) => {
            let item_type = map_string(object, "type");
            let is_tool_call = matches!(item_type.as_str(), "function_call" | "custom_tool_call");
            if is_tool_call
                && object.get("namespace").and_then(Value::as_str)
                    == Some(CODEX_OPTIMIZED_COLLABORATION_NAMESPACE)
            {
                object.insert(
                    "namespace".to_owned(),
                    Value::String(CODEX_COLLABORATION_NAMESPACE.to_owned()),
                );
                changed = true;
            }
            if let Some(name) = object
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                if item_type == "namespace" && name == CODEX_OPTIMIZED_COLLABORATION_NAMESPACE {
                    object.insert(
                        "name".to_owned(),
                        Value::String(CODEX_COLLABORATION_NAMESPACE.to_owned()),
                    );
                    changed = true;
                } else if is_tool_call
                    && name.starts_with(CODEX_OPTIMIZED_COLLABORATION_NAME_PREFIX)
                {
                    object.insert(
                        "name".to_owned(),
                        Value::String(format!(
                            "{CODEX_COLLABORATION_NAMESPACE}__{}",
                            &name[CODEX_OPTIMIZED_COLLABORATION_NAME_PREFIX.len()..]
                        )),
                    );
                    changed = true;
                }
            }
            for (key, child) in object {
                let opaque = key == "arguments"
                    || key == "input"
                    || (key == "output"
                        && matches!(
                            item_type.as_str(),
                            "function_call_output" | "custom_tool_call_output"
                        ));
                if !opaque {
                    changed |= restore_collaboration_value(child);
                }
            }
        }
        _ => {}
    }
    changed
}

#[must_use]
pub fn format_spawn_agent_models(models: &[SpawnAgentModel]) -> String {
    let mut lines = Vec::new();
    for model in models {
        let id = model.id.split_whitespace().collect::<Vec<_>>().join(" ");
        if id.is_empty() {
            continue;
        }
        let mut line = format!("- {}: ", markdown_code(&id));
        let description = model
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !description.is_empty() {
            line.push_str(&description);
            if !description.ends_with(['.', '!', '?']) {
                line.push('.');
            }
        }
        if !model.reasoning_efforts.is_empty() {
            if !line.ends_with(' ') {
                line.push(' ');
            }
            line.push_str("Reasoning efforts: ");
            line.push_str(
                &model
                    .reasoning_efforts
                    .iter()
                    .map(|effort| {
                        if *effort == model.default_reasoning_effort {
                            format!("{effort} (default)")
                        } else {
                            effort.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            line.push('.');
        }
        if !model.service_tiers.is_empty() {
            if !line.ends_with(' ') {
                line.push(' ');
            }
            line.push_str("Service tiers: ");
            line.push_str(&model.service_tiers.join(", "));
            line.push('.');
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn markdown_code(value: &str) -> String {
    if value.contains('`') {
        format!("`` {value} ``")
    } else {
        format!("`{value}`")
    }
}

#[must_use]
pub fn replace_spawn_agent_models(description: &str, model_list: &str) -> String {
    if model_list.is_empty() {
        return description.to_owned();
    }
    let (cleaned, indent) = remove_spawn_agent_model_sections(description);
    let section = format!("{indent}{CODEX_SPAWN_AGENT_MODELS_HEADING}\n{model_list}\n");
    if let Some(marker) = cleaned.find(CODEX_SPAWN_AGENT_DESCRIPTION_MARKER) {
        let line_start = cleaned[..marker].rfind('\n').map_or(0, |index| index + 1);
        return format!(
            "{}{}{}",
            &cleaned[..line_start],
            section,
            &cleaned[line_start..]
        );
    }
    let separator = if cleaned.is_empty() || cleaned.ends_with('\n') {
        ""
    } else {
        "\n\n"
    };
    format!("{cleaned}{separator}{}", section.trim_end_matches('\n'))
}

fn remove_spawn_agent_model_sections(description: &str) -> (String, String) {
    let mut output = Vec::new();
    let mut indent = String::new();
    let lines: Vec<_> = description.split_inclusive('\n').collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.trim() != CODEX_SPAWN_AGENT_MODELS_HEADING {
            output.push(line);
            index += 1;
            continue;
        }
        if indent.is_empty() {
            indent = line[..line.find(CODEX_SPAWN_AGENT_MODELS_HEADING).unwrap_or(0)].to_owned();
        }
        index += 1;
        while index < lines.len() && lines[index].trim().starts_with("- ") {
            index += 1;
        }
    }
    (output.concat(), indent)
}

fn map_string(values: &ModelMap, key: &str) -> String {
    values
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_owned()
}

fn map_integer(values: &ModelMap, key: &str) -> i64 {
    values.get(key).and_then(Value::as_i64).unwrap_or_default()
}

#[cfg(test)]
#[path = "optimize_multi_agent_v2_test.rs"]
mod optimize_multi_agent_v2_test;
