// ref: internal/registry/model_definitions.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const STATIC_MODELS_JSON: &str = include_str!("models/models.json");
/// Internal tri-state native metadata; public model lists omit this field.
/// ref: internal/registry/model_registry.go @ d7914afd
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct NativeCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_search: Option<bool>,
}

/// Lightweight capability view retained for the translated thinking pipeline.
/// The full dynamically owned wire model is [`RegistryModelInfo`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThinkingSupport {
    pub min: Option<u64>,
    pub max: Option<u64>,
    pub zero_allowed: bool,
    pub dynamic_allowed: bool,
    pub levels: &'static [&'static str],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelInfo {
    pub id: &'static str,
    pub provider_type: &'static str,
    pub user_defined: bool,
    pub max_completion_tokens: usize,
    pub thinking: Option<ThinkingSupport>,
}

/// Complete owned equivalent of upstream `ModelInfo`. Dynamic catalogs and
/// client registrations use this type; clones are deep and cannot mutate the
/// registry's snapshots.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryModelInfo {
    pub id: String,
    #[serde(default = "default_model_object")]
    pub object: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub owned_by: String,
    #[serde(rename = "type", default)]
    pub provider_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(
        default,
        rename = "inputTokenLimit",
        skip_serializing_if = "is_zero_usize"
    )]
    pub input_token_limit: usize,
    #[serde(
        default,
        rename = "outputTokenLimit",
        skip_serializing_if = "is_zero_usize"
    )]
    pub output_token_limit: usize,
    #[serde(
        default,
        rename = "supportedGenerationMethods",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub supported_generation_methods: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub context_length: usize,
    /// Internal-only Codex catalog override (`json:"-"` upstream).
    #[serde(skip)]
    pub max_context_length: usize,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub max_completion_tokens: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_parameters: Vec<String>,
    #[serde(
        default,
        rename = "supportedInputModalities",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub supported_input_modalities: Vec<String>,
    #[serde(
        default,
        rename = "supportedOutputModalities",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub supported_output_modalities: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub supports_web_search: bool,
    /// Decode internal catalog fields, but never expose them in public JSON.
    /// ref: internal/registry/model_registry.go:107-121 @ d7914afd
    #[serde(default, skip_serializing)]
    pub support_configuration_update: bool,
    #[serde(default, skip_serializing)]
    pub native_capabilities: Option<NativeCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<RegistryThinkingSupport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ModelConfig>,
    /// Configuration-origin marker (`json:"-"` upstream).
    #[serde(skip)]
    pub user_defined: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryThinkingSupport {
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub min: i64,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub max: i64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub zero_allowed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dynamic_allowed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelConfig {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub override_header: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct StaticModelsCatalog {
    #[serde(default)]
    pub claude: Vec<RegistryModelInfo>,
    #[serde(default)]
    pub gemini: Vec<RegistryModelInfo>,
    #[serde(default)]
    pub vertex: Vec<RegistryModelInfo>,
    #[serde(default)]
    pub aistudio: Vec<RegistryModelInfo>,
    #[serde(rename = "codex-free", default)]
    pub codex_free: Vec<RegistryModelInfo>,
    #[serde(rename = "codex-team", default)]
    pub codex_team: Vec<RegistryModelInfo>,
    #[serde(rename = "codex-plus", default)]
    pub codex_plus: Vec<RegistryModelInfo>,
    #[serde(rename = "codex-pro", default)]
    pub codex_pro: Vec<RegistryModelInfo>,
    #[serde(default)]
    pub kimi: Vec<RegistryModelInfo>,
    #[serde(default)]
    pub antigravity: Vec<RegistryModelInfo>,
    #[serde(default)]
    pub xai: Vec<RegistryModelInfo>,
    #[serde(default)]
    pub meta: Vec<RegistryModelInfo>,
    #[serde(default)]
    pub devin: Vec<RegistryModelInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StaticModelCatalogError {
    InvalidCatalog(String),
}

impl std::fmt::Display for StaticModelCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCatalog(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for StaticModelCatalogError {}

const LEVELS_46: &[&str] = &["low", "medium", "high", "max"];
const LEVELS_CURRENT: &[&str] = &["low", "medium", "high", "xhigh", "max"];

pub fn parse_models_catalog(data: &[u8]) -> Result<StaticModelsCatalog, StaticModelCatalogError> {
    let catalog: StaticModelsCatalog = serde_json::from_slice(data).map_err(|error| {
        StaticModelCatalogError::InvalidCatalog(format!("decode models catalog: {error}"))
    })?;
    validate_models_catalog(&catalog)?;
    Ok(catalog)
}

pub fn embedded_models_catalog() -> Result<StaticModelsCatalog, StaticModelCatalogError> {
    parse_models_catalog(STATIC_MODELS_JSON.as_bytes())
}

pub fn validate_models_catalog(
    catalog: &StaticModelsCatalog,
) -> Result<(), StaticModelCatalogError> {
    for (section, models) in [
        ("claude", &catalog.claude),
        ("gemini", &catalog.gemini),
        ("vertex", &catalog.vertex),
        ("aistudio", &catalog.aistudio),
        ("codex-free", &catalog.codex_free),
        ("codex-team", &catalog.codex_team),
        ("codex-plus", &catalog.codex_plus),
        ("codex-pro", &catalog.codex_pro),
        ("kimi", &catalog.kimi),
        ("antigravity", &catalog.antigravity),
        ("xai", &catalog.xai),
        ("meta", &catalog.meta),
        ("devin", &catalog.devin),
    ] {
        let mut seen = HashSet::with_capacity(models.len());
        for (index, model) in models.iter().enumerate() {
            let id = model.id.trim();
            if id.is_empty() {
                return Err(StaticModelCatalogError::InvalidCatalog(format!(
                    "{section}[{index}] has empty id"
                )));
            }
            if !seen.insert(id) {
                return Err(StaticModelCatalogError::InvalidCatalog(format!(
                    "{section} contains duplicate model id {id:?}"
                )));
            }
        }
    }
    Ok(())
}

pub fn models_for_channel(
    catalog: &StaticModelsCatalog,
    channel: &str,
) -> Option<Vec<RegistryModelInfo>> {
    let key = channel.trim().to_ascii_lowercase();
    let models = match key.as_str() {
        "claude" => catalog.claude.clone(),
        "gemini" => catalog.gemini.clone(),
        "vertex" => catalog.vertex.clone(),
        "aistudio" => catalog.aistudio.clone(),
        "codex-free" => with_codex_builtins(catalog.codex_free.clone()),
        "codex-team" => with_codex_builtins(catalog.codex_team.clone()),
        "codex-plus" => with_codex_builtins(catalog.codex_plus.clone()),
        "codex" | "codex-pro" => with_codex_builtins(catalog.codex_pro.clone()),
        "kimi" => catalog.kimi.clone(),
        "antigravity" => catalog.antigravity.clone(),
        "xai" | "x-ai" | "grok" => with_xai_builtins(catalog.xai.clone()),
        "meta" | "muse" => catalog.meta.clone(),
        "gemini-interactions" => catalog.gemini.clone(),
        _ => return None,
    };
    Some(models)
}

pub fn lookup_static_registry_model_info(
    catalog: &StaticModelsCatalog,
    model_id: &str,
) -> Option<RegistryModelInfo> {
    if model_id.is_empty() {
        return None;
    }
    [
        &catalog.claude,
        &catalog.gemini,
        &catalog.vertex,
        &catalog.aistudio,
        &catalog.codex_pro,
        &catalog.kimi,
        &catalog.antigravity,
        &catalog.xai,
        &catalog.devin,
        &catalog.meta,
    ]
    .into_iter()
    .flatten()
    .find(|model| model.id == model_id)
    .cloned()
}

pub fn model_override_headers(
    catalog: &StaticModelsCatalog,
    model_id: &str,
) -> Option<BTreeMap<String, String>> {
    let info = lookup_static_registry_model_info(catalog, model_id)?;
    let headers = info.config?.override_header;
    let filtered = headers
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_owned();
            (!key.is_empty()).then_some((key, value))
        })
        .collect::<BTreeMap<_, _>>();
    (!filtered.is_empty()).then_some(filtered)
}

pub fn with_codex_builtins(models: Vec<RegistryModelInfo>) -> Vec<RegistryModelInfo> {
    upsert_model_infos(models, codex_builtins())
}

pub fn with_xai_builtins(models: Vec<RegistryModelInfo>) -> Vec<RegistryModelInfo> {
    upsert_model_infos(models, xai_builtins())
}

fn upsert_model_infos(
    models: Vec<RegistryModelInfo>,
    extras: Vec<RegistryModelInfo>,
) -> Vec<RegistryModelInfo> {
    let mut seen = HashSet::new();
    let extras = extras
        .into_iter()
        .filter(|model| !model.id.trim().is_empty())
        .filter(|model| seen.insert(model.id.trim().to_ascii_lowercase()))
        .collect::<Vec<_>>();
    if extras.is_empty() {
        return models;
    }
    let extra_ids = extras
        .iter()
        .map(|model| model.id.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    models
        .into_iter()
        .filter(|model| {
            let id = model.id.trim();
            !id.is_empty() && !extra_ids.contains(&id.to_ascii_lowercase())
        })
        .chain(extras)
        .collect()
}

fn codex_builtins() -> Vec<RegistryModelInfo> {
    [
        ("gpt-image-1.5", "GPT Image 1.5"),
        ("gpt-image-2", "GPT Image 2"),
        ("gpt-image-2.5-flare", "GPT Image 2.5 Flare"),
        ("gpt-image-2.5-sunburst", "GPT Image 2.5 Sunburst"),
        ("gpt-image-2.5", "GPT Image 2.5"),
    ]
    .into_iter()
    .map(|(id, display_name)| RegistryModelInfo {
        id: id.to_owned(),
        object: "model".to_owned(),
        created: 1_704_067_200,
        owned_by: "openai".to_owned(),
        provider_type: "openai".to_owned(),
        display_name: display_name.to_owned(),
        version: id.to_owned(),
        ..RegistryModelInfo::default()
    })
    .collect()
}

fn xai_builtins() -> Vec<RegistryModelInfo> {
    [
        (
            "grok-imagine-image",
            "Grok Imagine Image",
            "xAI Grok image generation model.",
        ),
        (
            "grok-imagine-image-quality",
            "Grok Imagine Image Quality",
            "xAI Grok higher-fidelity image generation model.",
        ),
        (
            "grok-imagine-image-2.0",
            "Grok Imagine Image 2.0",
            "xAI Grok image generation model.",
        ),
        (
            "grok-imagine-video",
            "Grok Imagine Video",
            "xAI Grok video generation model.",
        ),
        (
            "grok-imagine-video-1.5",
            "Grok Imagine Video 1.5",
            "xAI Grok video generation model.",
        ),
        (
            "grok-imagine-video-1.5-preview",
            "Grok Imagine Video 1.5 Preview",
            "Compatibility alias for the xAI Grok video generation model.",
        ),
    ]
    .into_iter()
    .map(|(id, display_name, description)| RegistryModelInfo {
        id: id.to_owned(),
        object: "model".to_owned(),
        created: if id == "grok-imagine-image-2.0" {
            1_786_060_800
        } else {
            1_735_689_600
        },
        owned_by: "xai".to_owned(),
        provider_type: "xai".to_owned(),
        display_name: display_name.to_owned(),
        name: id.to_owned(),
        description: description.to_owned(),
        ..RegistryModelInfo::default()
    })
    .collect()
}

pub fn static_model_definitions_by_channel(
    channel: &str,
) -> Result<Option<Value>, StaticModelCatalogError> {
    let catalog = embedded_models_catalog()?;
    let Some(models) = models_for_channel(&catalog, channel) else {
        return Ok(None);
    };
    serde_json::to_value(models)
        .map(Some)
        .map_err(|error| StaticModelCatalogError::InvalidCatalog(error.to_string()))
}

pub fn antigravity_web_search_model_for(
    registry: &super::ModelRegistry,
    model_id: &str,
) -> Option<String> {
    let normalized = normalize_capability_model_id(model_id);
    if normalized.is_empty() {
        return None;
    }
    registry
        .available_models_by_provider("antigravity")
        .into_iter()
        .find(|model| {
            normalize_capability_model_id(&model.id) == normalized && model.supports_web_search
        })
        .map(|_| normalized)
}

fn normalize_capability_model_id(model_id: &str) -> String {
    let mut normalized = model_id.trim().to_ascii_lowercase();
    if let Some(open) = normalized.rfind('(') {
        if normalized.ends_with(')') {
            normalized.truncate(open);
            normalized = normalized.trim().to_owned();
        }
    }
    normalized
}

pub fn lookup_model_info(model_name: &str, provider_type: &str) -> Option<ModelInfo> {
    if !provider_type.eq_ignore_ascii_case("claude") {
        return None;
    }
    let normalized = normalize_capability_model_id(model_name);
    let catalog = embedded_models_catalog().ok()?;
    let model = catalog
        .claude
        .iter()
        .find(|model| model.id.eq_ignore_ascii_case(&normalized))?;
    let id = static_claude_id(&model.id)?;
    let thinking = model.thinking.as_ref().map(|support| ThinkingSupport {
        min: u64::try_from(support.min).ok().filter(|value| *value != 0),
        max: u64::try_from(support.max).ok().filter(|value| *value != 0),
        zero_allowed: support.zero_allowed,
        dynamic_allowed: support.dynamic_allowed,
        levels: static_levels(&support.levels),
    });
    Some(ModelInfo {
        id,
        provider_type: "claude",
        user_defined: model.user_defined,
        max_completion_tokens: model.max_completion_tokens,
        thinking,
    })
}

fn static_claude_id(id: &str) -> Option<&'static str> {
    match id {
        "claude-sonnet-4-6" => Some("claude-sonnet-4-6"),
        "claude-opus-4-6" => Some("claude-opus-4-6"),
        "claude-opus-4-7" => Some("claude-opus-4-7"),
        "claude-opus-4-8" => Some("claude-opus-4-8"),
        "claude-opus-5" => Some("claude-opus-5"),
        "claude-sonnet-5" => Some("claude-sonnet-5"),
        "claude-fable-5" => Some("claude-fable-5"),
        _ => None,
    }
}

fn static_levels(levels: &[String]) -> &'static [&'static str] {
    let normalized = levels.iter().map(String::as_str).collect::<Vec<_>>();
    if normalized == LEVELS_46 {
        LEVELS_46
    } else if normalized == LEVELS_CURRENT {
        LEVELS_CURRENT
    } else {
        &[]
    }
}

fn default_model_object() -> String {
    "model".to_owned()
}

fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}
