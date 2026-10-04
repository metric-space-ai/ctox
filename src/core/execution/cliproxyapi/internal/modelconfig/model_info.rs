// ref: internal/modelconfig/model_info.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde::{Deserialize, Serialize};

use crate::internal::{registry, thinking};

/// An owned, wire-compatible copy of a model's configured thinking support.
///
/// Upstream can clone a `[]string` in its registry structure. CTOX's current
/// static registry deliberately uses static string slices, so modelconfig owns
/// the normalized values instead of leaking configured strings to obtain a
/// `'static` lifetime.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ThinkingSupport {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub min: i64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub max: i64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub zero_allowed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dynamic_allowed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<String>,
}

/// Private capability snapshot returned for one configured model.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub provider_type: String,
    pub user_defined: bool,
    /// Private per-selected-model capability, matching registry.ModelInfo.IsCompat.
    /// Static catalog entries default to false.
    pub is_compat: bool,
    pub input_token_limit: usize,
    pub output_token_limit: usize,
    pub context_length: usize,
    pub max_completion_tokens: usize,
    pub thinking: Option<ThinkingSupport>,
    pub native_capabilities: Option<NativeCapabilities>,
    pub support_configuration_update: bool,
}

pub use crate::internal::registry::NativeCapabilities;

/// Execution-only selected credential model options and upstream defaults.
/// ref: internal/config/config_types.go:892-928 @ d7914afd
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HomeModelOptions {
    pub name: String,
    pub alias: String,
    pub display_name: String,
    pub max_context_length: usize,
    pub force_mapping: bool,
    pub image: bool,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    pub is_compat: bool,
    pub use_max_completion_tokens: bool,
    pub thinking: Option<ThinkingSupport>,
}

/// Returns a private capability snapshot for a configured model.
///
/// Static capabilities come from the suffix-free upstream name, while an
/// explicit configured thinking capability takes precedence.
pub fn resolve_model_info(
    name: &str,
    model_type: &str,
    support: Option<&ThinkingSupport>,
) -> ModelInfo {
    let trimmed_name = name.trim();
    let base_name = thinking::parse_suffix(trimmed_name).model_name;
    let mut info = registry::embedded_models_catalog()
        .ok()
        .and_then(|catalog| registry::lookup_static_registry_model_info(&catalog, base_name.trim()))
        .map(ModelInfo::from)
        .unwrap_or_default();
    info.id = trimmed_name.to_owned();
    info.provider_type = model_type.trim().to_owned();
    if let Some(support) = support {
        info.thinking = Some(normalize_thinking_support(support));
    }
    info.user_defined = false;
    info
}

/// Clones and normalizes configured reasoning levels.
pub fn normalize_thinking_support(raw: &ThinkingSupport) -> ThinkingSupport {
    let mut normalized = raw.clone();
    normalized.levels.clear();
    for value in &raw.levels {
        let level = value.trim().to_ascii_lowercase();
        if level.is_empty() {
            continue;
        }
        match level.as_str() {
            "none" => normalized.zero_allowed = true,
            "auto" => normalized.dynamic_allowed = true,
            _ => {}
        }
        if !normalized.levels.contains(&level) {
            normalized.levels.push(level);
        }
    }
    normalized
}

impl From<registry::ModelInfo> for ModelInfo {
    fn from(info: registry::ModelInfo) -> Self {
        Self {
            id: info.id.to_owned(),
            provider_type: info.provider_type.to_owned(),
            user_defined: info.user_defined,
            is_compat: false,
            max_completion_tokens: info.max_completion_tokens,
            thinking: info.thinking.map(ThinkingSupport::from),
            ..Self::default()
        }
    }
}

impl From<registry::RegistryModelInfo> for ModelInfo {
    fn from(info: registry::RegistryModelInfo) -> Self {
        Self {
            id: info.id,
            provider_type: info.provider_type,
            user_defined: info.user_defined,
            is_compat: false,
            input_token_limit: info.input_token_limit,
            output_token_limit: info.output_token_limit,
            context_length: info.context_length,
            max_completion_tokens: info.max_completion_tokens,
            thinking: info.thinking.map(|support| ThinkingSupport {
                min: support.min,
                max: support.max,
                zero_allowed: support.zero_allowed,
                dynamic_allowed: support.dynamic_allowed,
                levels: support.levels,
            }),
            native_capabilities: info.native_capabilities,
            support_configuration_update: info.support_configuration_update,
        }
    }
}

impl From<registry::ThinkingSupport> for ThinkingSupport {
    fn from(support: registry::ThinkingSupport) -> Self {
        Self {
            min: support
                .min
                .and_then(|value| i64::try_from(value).ok())
                .unwrap_or_default(),
            max: support
                .max
                .and_then(|value| i64::try_from(value).ok())
                .unwrap_or_default(),
            zero_allowed: support.zero_allowed,
            dynamic_allowed: support.dynamic_allowed,
            levels: support
                .levels
                .iter()
                .map(|level| (*level).to_owned())
                .collect(),
        }
    }
}

const fn is_false(value: &bool) -> bool {
    !*value
}

const fn is_zero(value: &i64) -> bool {
    *value == 0
}
