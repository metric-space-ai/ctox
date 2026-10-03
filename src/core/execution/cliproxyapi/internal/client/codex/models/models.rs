// ref: internal/client/codex/models/models.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;
use serde_json::{Map, Value};

const DEFAULT_TEMPLATE_SLUG: &str = "gpt-5.5";
const IMAGE_MODEL_TYPE: &str = "openai-image";
const ALLOWED_REASONING_LEVELS: [&str; 8] = [
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

pub type ModelMap = Map<String, Value>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModelMetadata {
    pub display_name: String,
    pub description: String,
    pub context_length: u64,
    pub model_type: String,
    pub supported_input_modalities: Vec<String>,
    pub thinking_levels: Vec<String>,
}

pub trait ModelMetadataSource {
    fn lookup(&self, model_id: &str) -> Option<ModelMetadata>;
}

impl<F> ModelMetadataSource for F
where
    F: Fn(&str) -> Option<ModelMetadata>,
{
    fn lookup(&self, model_id: &str) -> Option<ModelMetadata> {
        self(model_id)
    }
}

pub trait ProvidersForModel {
    fn providers(&self, model_id: &str) -> Vec<String>;
}

impl<F> ProvidersForModel for F
where
    F: Fn(&str) -> Vec<String>,
{
    fn providers(&self, model_id: &str) -> Vec<String> {
        self(model_id)
    }
}

/// True only when every actual routing candidate supports freeform patches.
/// An injected resolver must return false for unknown support.
pub trait ApplyPatchCapabilityForModel {
    fn supports_apply_patch(&self, model_id: &str) -> bool;
}

impl<F> ApplyPatchCapabilityForModel for F
where
    F: Fn(&str) -> bool,
{
    fn supports_apply_patch(&self, model_id: &str) -> bool {
        self(model_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogError(String);

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CatalogError {}

#[derive(Clone, Debug)]
pub struct CodexModelCatalog {
    revision: u64,
    templates: BTreeMap<String, ModelMap>,
    default_template: ModelMap,
}

#[derive(Deserialize)]
struct CatalogPayload {
    models: Vec<ModelMap>,
}

impl CodexModelCatalog {
    pub fn parse(raw: &[u8], revision: u64) -> Result<Self, CatalogError> {
        let payload: CatalogPayload = serde_json::from_slice(raw)
            .map_err(|error| CatalogError(format!("decode Codex client model catalog: {error}")))?;
        let mut templates = BTreeMap::new();
        for model in payload.models {
            let slug = string_value(&model, "slug");
            if !slug.is_empty() {
                templates.insert(slug, model);
            }
        }
        let default_template = templates
            .get(DEFAULT_TEMPLATE_SLUG)
            .cloned()
            .ok_or_else(|| {
                CatalogError("Codex client catalog has no gpt-5.5 template".to_owned())
            })?;
        Ok(Self {
            revision,
            templates,
            default_template,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn build_response(
        &self,
        available_models: &[ModelMap],
        metadata: &dyn ModelMetadataSource,
        providers: Option<&dyn ProvidersForModel>,
        optimize_multi_agent_v2: bool,
    ) -> Value {
        Value::Object(Map::from_iter([(
            "models".to_owned(),
            Value::Array(
                self.build_models(
                    available_models,
                    metadata,
                    providers,
                    optimize_multi_agent_v2,
                )
                .into_iter()
                .map(Value::Object)
                .collect(),
            ),
        )]))
    }

    #[must_use]
    pub fn build_models(
        &self,
        available_models: &[ModelMap],
        metadata: &dyn ModelMetadataSource,
        providers: Option<&dyn ProvidersForModel>,
        optimize_multi_agent_v2: bool,
    ) -> Vec<ModelMap> {
        self.build_models_with_apply_patch_capability(
            available_models,
            metadata,
            providers,
            optimize_multi_agent_v2,
            None,
        )
    }

    #[must_use]
    pub fn build_models_with_apply_patch_capability(
        &self,
        available_models: &[ModelMap],
        metadata: &dyn ModelMetadataSource,
        providers: Option<&dyn ProvidersForModel>,
        optimize_multi_agent_v2: bool,
        apply_patch_capability: Option<&dyn ApplyPatchCapabilityForModel>,
    ) -> Vec<ModelMap> {
        let mut result = Vec::with_capacity(available_models.len());
        for model in available_models {
            let id = string_value(model, "id");
            if id.is_empty() {
                continue;
            }
            let template_model = self.templates.contains_key(&id);
            let mut entry = self
                .templates
                .get(&id)
                .cloned()
                .unwrap_or_else(|| self.default_template.clone());
            if template_model {
                apply_display_name(&mut entry, model);
                apply_max_context_length_override(&mut entry, model);
            } else {
                apply_model_metadata(
                    &mut entry,
                    &id,
                    model,
                    metadata.lookup(&id).as_ref(),
                    optimize_multi_agent_v2,
                );
            }
            apply_search_tool_support(&mut entry, &id, template_model, providers);
            sanitize_reasoning_metadata(&mut entry);
            apply_visibility_override(&mut entry, &id);
            apply_patch_tool_capability(&mut entry, &id, apply_patch_capability);
            result.push(entry);
        }
        apply_non_template_priorities(&mut result, &self.templates);
        result.sort_by_key(model_priority);
        result
    }
}

fn apply_display_name(entry: &mut ModelMap, model: &ModelMap) {
    let display_name = string_value(model, "display_name");
    if !display_name.is_empty() {
        entry.insert("display_name".to_owned(), Value::String(display_name));
    }
}

fn apply_max_context_length_override(entry: &mut ModelMap, model: &ModelMap) {
    if let Some(maximum) = positive_integer(model, "max_context_length") {
        entry.insert("context_window".to_owned(), maximum.into());
        entry.insert("max_context_window".to_owned(), maximum.into());
    }
}

fn apply_search_tool_support(
    entry: &mut ModelMap,
    id: &str,
    template_model: bool,
    providers: Option<&dyn ProvidersForModel>,
) {
    if !entry
        .get("supports_search_tool")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return;
    }
    let supported = template_model
        && providers.is_none_or(|source| {
            let values = source.providers(id);
            !values.is_empty()
                && values
                    .iter()
                    .all(|provider| provider.trim().eq_ignore_ascii_case("codex"))
        });
    entry.insert("supports_search_tool".to_owned(), Value::Bool(supported));
}

fn apply_model_metadata(
    entry: &mut ModelMap,
    id: &str,
    model: &ModelMap,
    metadata: Option<&ModelMetadata>,
    optimize_multi_agent_v2: bool,
) {
    let mut display_name = string_value(model, "display_name");
    let mut description = string_value(model, "description");
    let mut context_window = positive_integer(model, "context_length").unwrap_or(0);
    if let Some(metadata) = metadata {
        if !metadata.display_name.trim().is_empty() {
            display_name = metadata.display_name.trim().to_owned();
        }
        if !metadata.description.trim().is_empty() {
            description = metadata.description.trim().to_owned();
        }
        if metadata.context_length > 0 {
            context_window = metadata.context_length;
        }
        if metadata.model_type == IMAGE_MODEL_TYPE {
            entry.insert("visibility".to_owned(), Value::String("hide".to_owned()));
            entry.remove("input_modalities");
            entry.remove("supports_image_detail_original");
        } else {
            apply_input_modalities(entry, &metadata.supported_input_modalities);
        }
        apply_thinking_metadata(entry, &metadata.thinking_levels);
    }
    if let Some(maximum) = positive_integer(model, "max_context_length") {
        context_window = maximum;
    }
    if display_name.is_empty() {
        display_name = id.to_owned();
    }
    if description.is_empty() {
        description = id.to_owned();
    }
    entry.insert("slug".to_owned(), Value::String(id.to_owned()));
    entry.insert("display_name".to_owned(), Value::String(display_name));
    entry.insert("description".to_owned(), Value::String(description));
    entry.insert("prefer_websockets".to_owned(), Value::Bool(false));
    if optimize_multi_agent_v2 {
        entry.insert(
            "multi_agent_version".to_owned(),
            Value::String("v2".to_owned()),
        );
    }
    entry.insert("service_tiers".to_owned(), Value::Array(Vec::new()));
    for key in ["upgrade", "availability_nux"] {
        entry.remove(key);
    }
    if context_window > 0 {
        entry.insert("context_window".to_owned(), context_window.into());
        entry.insert("max_context_window".to_owned(), context_window.into());
    }
    let base_instructions = string_value(model, "base_instructions");
    if !base_instructions.is_empty() {
        entry.insert(
            "base_instructions".to_owned(),
            Value::String(base_instructions),
        );
    }
    if let Some(plans) = model.get("available_in_plans") {
        entry.insert("available_in_plans".to_owned(), plans.clone());
    }
}

fn apply_visibility_override(entry: &mut ModelMap, id: &str) {
    let target = id.trim().split_once('/').map_or(id.trim(), |(_, suffix)| suffix.trim());
    if is_image_or_video_model(target) {
        entry.insert("visibility".to_owned(), Value::String("hide".to_owned()));
    }
}

fn is_image_or_video_model(id: &str) -> bool {
    matches!(
        id,
        "grok-imagine-image-quality"
            | "gpt-image-1.5"
            | "gpt-image-2"
            | "gpt-image-2.5-flare"
            | "gpt-image-2.5-sunburst"
            | "gpt-image-2.5"
            | "grok-imagine-image"
            | "grok-imagine-image-2.0"
            | "grok-imagine-video"
            | "grok-imagine-video-1.5"
            | "grok-imagine-video-1.5-preview"
    )
}

// ref: internal/client/codex/models/apply_patch.go:8-55 @ 2044a01f
fn apply_patch_tool_capability(
    entry: &mut ModelMap,
    id: &str,
    capability: Option<&dyn ApplyPatchCapabilityForModel>,
) {
    let template_supported = string_value(entry, "apply_patch_tool_type") == "freeform";
    entry.insert("apply_patch_tool_type".to_owned(), Value::Null);
    let base_id = id.trim().to_ascii_lowercase();
    let base_id = base_id.rsplit('/').next().unwrap_or(&base_id).trim();
    if is_image_or_video_model(base_id) {
        return;
    }
    let modalities = entry.get("input_modalities").and_then(Value::as_array);
    let has_modalities = modalities.is_some_and(|values| !values.is_empty());
    let supports_text = modalities.is_some_and(|values| values.iter().any(|value| value == "text"));
    if !supports_text && (has_modalities || string_value(entry, "visibility") == "hide") {
        return;
    }
    let supported = capability.map_or(template_supported, |resolver| resolver.supports_apply_patch(id.trim()));
    if supported {
        entry.insert("apply_patch_tool_type".to_owned(), Value::String("freeform".to_owned()));
    }
}

fn apply_input_modalities(entry: &mut ModelMap, modalities: &[String]) {
    let mut seen = BTreeSet::new();
    let mut accepted = Vec::new();
    for modality in modalities {
        let modality = modality.trim().to_ascii_lowercase();
        if matches!(modality.as_str(), "text" | "image") && seen.insert(modality.clone()) {
            accepted.push(Value::String(modality));
        }
    }
    if accepted.is_empty() {
        return;
    }
    let supports_image = accepted.iter().any(|value| value == "image");
    entry.insert("input_modalities".to_owned(), Value::Array(accepted));
    if supports_image {
        entry.insert(
            "supports_image_detail_original".to_owned(),
            Value::Bool(true),
        );
    } else {
        entry.remove("supports_image_detail_original");
    }
}

fn apply_thinking_metadata(entry: &mut ModelMap, raw_levels: &[String]) {
    let levels: Vec<_> = raw_levels
        .iter()
        .filter_map(|level| normalize_reasoning_level(level))
        .map(|level| {
            Value::Object(Map::from_iter([
                ("effort".to_owned(), Value::String(level.clone())),
                (
                    "description".to_owned(),
                    Value::String(reasoning_description(&level).to_owned()),
                ),
            ]))
        })
        .collect();
    if levels.is_empty() {
        return;
    }
    let default = levels
        .iter()
        .filter_map(|entry| entry.get("effort").and_then(Value::as_str))
        .find(|effort| *effort == "medium")
        .or_else(|| {
            levels
                .iter()
                .filter_map(|entry| entry.get("effort").and_then(Value::as_str))
                .find(|effort| *effort != "none")
        })
        .or_else(|| levels[0].get("effort").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned();
    entry.insert(
        "supported_reasoning_levels".to_owned(),
        Value::Array(levels),
    );
    entry.insert("default_reasoning_level".to_owned(), Value::String(default));
}

fn sanitize_reasoning_metadata(entry: &mut ModelMap) {
    let Some(raw_levels) = entry
        .get("supported_reasoning_levels")
        .and_then(Value::as_array)
    else {
        return;
    };
    let mut allowed = BTreeSet::new();
    let levels: Vec<_> = raw_levels
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|level| {
            let effort = normalize_reasoning_level(&string_value(level, "effort"))?;
            allowed.insert(effort.clone());
            let mut level = level.clone();
            level.insert("effort".to_owned(), Value::String(effort));
            Some(Value::Object(level))
        })
        .collect();
    if levels.is_empty() {
        entry.remove("supported_reasoning_levels");
        entry.remove("default_reasoning_level");
        return;
    }
    let current = entry
        .get("default_reasoning_level")
        .and_then(Value::as_str)
        .and_then(normalize_reasoning_level);
    let default = current
        .filter(|level| allowed.contains(level))
        .unwrap_or_else(|| {
            levels[0]
                .get("effort")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        });
    entry.insert(
        "supported_reasoning_levels".to_owned(),
        Value::Array(levels),
    );
    entry.insert("default_reasoning_level".to_owned(), Value::String(default));
}

fn normalize_reasoning_level(raw: &str) -> Option<String> {
    let level = raw.trim().to_ascii_lowercase();
    ALLOWED_REASONING_LEVELS
        .contains(&level.as_str())
        .then_some(level)
}

fn reasoning_description(level: &str) -> &str {
    match level {
        "none" => "No reasoning",
        "minimal" => "Fastest responses with minimal reasoning",
        "low" => "Fast responses with lighter reasoning",
        "medium" => "Balances speed and reasoning depth for everyday tasks",
        "high" => "Greater reasoning depth for complex problems",
        "xhigh" => "Extra high reasoning depth for complex problems",
        "max" => "Maximum available reasoning depth for complex problems",
        _ => level,
    }
}

fn apply_non_template_priorities(result: &mut [ModelMap], templates: &BTreeMap<String, ModelMap>) {
    let base = templates.values().map(model_priority).max().unwrap_or(0);
    let mut pending: Vec<_> = result
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let slug = string_value(entry, "slug");
            (!templates.contains_key(&slug)).then(|| {
                let display = string_value(entry, "display_name");
                (index, display.to_ascii_lowercase(), slug)
            })
        })
        .collect();
    pending.sort_by(|left, right| (&left.1, &left.2).cmp(&(&right.1, &right.2)));
    for (rank, (index, _, _)) in pending.into_iter().enumerate() {
        result[index].insert(
            "priority".to_owned(),
            (base + 100 * (rank as i64 + 1)).into(),
        );
    }
}

fn model_priority(model: &ModelMap) -> i64 {
    model.get("priority").and_then(Value::as_i64).unwrap_or(100)
}

fn string_value(model: &ModelMap, key: &str) -> String {
    model
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_owned()
}

fn positive_integer(model: &ModelMap, key: &str) -> Option<u64> {
    model
        .get(key)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
}
