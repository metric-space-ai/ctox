// ref: internal/registry/devin_models.go:18-488 @ d7914afd
// Port-Status: adapted_to_ctox — instance-owned store rather than package globals
// License: MIT (upstream); modifications AGPL-3.0-only

use super::model_definitions::with_devin_builtins;
use super::{CatalogLoad, RegistryModelInfo, RegistryThinkingSupport, StaticModelsCatalog};
use serde::Deserialize;
use std::{collections::HashSet, fmt, sync::RwLock};

pub const EMBEDDED_DEVIN_MODELS_JSON: &[u8] = include_bytes!("models/devin_models.json");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevinModelsError(String);
impl fmt::Display for DevinModelsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for DevinModelsError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DevinModelsSnapshot {
    pub models: Vec<RegistryModelInfo>,
    pub data: Vec<u8>,
    pub revision: u64,
}

/// Separate catalog authority, owned by the existing host model-catalog store.
/// Copies include raw bytes and revision from the same lock acquisition.
#[derive(Default)]
pub struct DevinModelsStore {
    state: RwLock<DevinModelsSnapshot>,
}
impl DevinModelsStore {
    pub fn from_embedded() -> Result<Self, DevinModelsError> {
        let store = Self::default();
        store.load(EMBEDDED_DEVIN_MODELS_JSON, "embed")?;
        Ok(store)
    }
    pub fn snapshot(&self) -> DevinModelsSnapshot {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    pub fn load(&self, data: &[u8], source: &str) -> Result<CatalogLoad, DevinModelsError> {
        let models = validate_devin_models_json(data)
            .map_err(|error| DevinModelsError(format!("{source}: {error}")))?;
        let models = with_devin_builtins(models);
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let changed = state.data != data;
        if changed {
            state.models = models;
            state.data = data.to_vec();
            state.revision = state.revision.saturating_add(1);
        }
        Ok(CatalogLoad {
            changed,
            revision: state.revision,
            changed_providers: if changed {
                vec!["devin".into()]
            } else {
                Vec::new()
            },
        })
    }
    /// Dynamic/embedded catalog, then main models.json, then hard-coded fallback.
    pub fn models(&self, catalog: &StaticModelsCatalog) -> Vec<RegistryModelInfo> {
        let models = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .models
            .clone();
        with_devin_builtins(if !models.is_empty() {
            models
        } else if !catalog.devin.is_empty() {
            catalog.devin.clone()
        } else {
            super::devin_builtin::static_devin_models()
        })
    }
    pub fn lookup(
        &self,
        model_id: &str,
        catalog: &StaticModelsCatalog,
    ) -> Option<RegistryModelInfo> {
        let normalized = model_id.trim().to_lowercase();
        let clean = normalized.strip_prefix("devin/").unwrap_or(&normalized);
        if clean.is_empty() {
            return None;
        }
        let models = self.models(catalog);
        if let Some(model) = models.iter().find(|model| {
            model
                .id
                .strip_prefix("devin/")
                .unwrap_or(&model.id)
                .eq_ignore_ascii_case(clean)
        }) {
            return Some(model.clone());
        }
        let (base, _) = split_devin_model_id(clean);
        if base == clean || base.is_empty() {
            return None;
        }
        models.into_iter().find(|model| {
            model
                .id
                .strip_prefix("devin/")
                .unwrap_or(&model.id)
                .eq_ignore_ascii_case(&base)
        })
    }
}

#[derive(Deserialize)]
struct DevinModelsPayload {
    #[serde(default)]
    devin: Option<Vec<Option<RegistryModelInfo>>>,
    #[serde(default)]
    models: Option<Vec<Option<RegistryModelInfo>>>,
}

pub fn validate_devin_models_json(data: &[u8]) -> Result<Vec<RegistryModelInfo>, DevinModelsError> {
    if data.iter().all(u8::is_ascii_whitespace) {
        return Err(DevinModelsError("empty Devin models payload".into()));
    }
    if let Ok(payload) = serde_json::from_slice::<DevinModelsPayload>(data) {
        let mut candidates = payload.devin.unwrap_or_default();
        if candidates.is_empty() {
            candidates = payload.models.unwrap_or_default();
        }
        if !candidates.is_empty() {
            return sanitize(candidates);
        }
    }
    if let Ok(models) = serde_json::from_slice::<Vec<Option<RegistryModelInfo>>>(data) {
        if !models.is_empty() {
            return sanitize(models);
        }
    }
    Err(DevinModelsError(
        "invalid Devin models JSON: expected non-empty 'devin'/'models' array or model list".into(),
    ))
}

fn sanitize(
    models: Vec<Option<RegistryModelInfo>>,
) -> Result<Vec<RegistryModelInfo>, DevinModelsError> {
    let mut seen = HashSet::with_capacity(models.len());
    let mut sanitized = Vec::with_capacity(models.len());
    for (index, model) in models.into_iter().enumerate() {
        let mut model =
            model.ok_or_else(|| DevinModelsError(format!("model at index {index} is null")))?;
        let id = model.id.trim().to_lowercase();
        if id.is_empty() {
            return Err(DevinModelsError(format!(
                "model at index {index} has empty id"
            )));
        }
        model.id = if id.starts_with("devin/") {
            id
        } else {
            format!("devin/{id}")
        };
        if !seen.insert(model.id.clone()) {
            return Err(DevinModelsError(format!(
                "duplicate model id: {:?}",
                model.id
            )));
        }
        sanitized.push(model);
    }
    Ok(aggregate(sanitized))
}

const COMPOUND_SUFFIXES: &[(&str, &str, &str)] = &[
    ("-low-fast", "low", ""),
    ("-medium-fast", "medium", ""),
    ("-high-fast", "high", ""),
    ("-xhigh-fast", "xhigh", ""),
    ("-max-fast", "max", ""),
    ("-none-fast", "none", ""),
    ("-low-priority", "low", ""),
    ("-medium-priority", "medium", ""),
    ("-high-priority", "high", ""),
    ("-xhigh-priority", "xhigh", ""),
    ("-max-priority", "max", ""),
    ("-none-priority", "none", ""),
    ("-thinking-1m", "", "-1m"),
    ("-thinking", "", ""),
    ("-max-1m", "max", "-1m"),
    ("-none-1m", "none", "-1m"),
];
const SIMPLE_EFFORTS: &[&str] = &["none", "minimal", "low", "medium", "high", "xhigh", "max"];
fn split_devin_model_id(id: &str) -> (String, String) {
    if id == "swe-1-6-slow" {
        return (id.into(), String::new());
    }
    if id == "swe-1-6-fast" {
        return ("swe-1-6".into(), String::new());
    }
    let lower = id.to_lowercase();
    for effort in SIMPLE_EFFORTS
        .iter()
        .copied()
        .chain(std::iter::once("thinking"))
    {
        let suffix = format!("_{effort}");
        if lower.ends_with(&suffix) {
            return (
                id[..id.len() - suffix.len()].into(),
                if effort == "thinking" { "high" } else { effort }.into(),
            );
        }
    }
    for (suffix, effort, readd) in COMPOUND_SUFFIXES {
        if let Some(base) = id.strip_suffix(*suffix) {
            return (format!("{base}{readd}"), (*effort).into());
        }
    }
    for effort in SIMPLE_EFFORTS {
        if let Some(base) = id.strip_suffix(&format!("-{effort}")) {
            return (base.into(), (*effort).into());
        }
    }
    (id.into(), String::new())
}

const DISPLAY_SUFFIXES: &[&str] = &[
    " Low Fast",
    " Medium Fast",
    " High Fast",
    " XHigh Fast",
    " Max Fast",
    " Low Thinking Fast",
    " Medium Thinking Fast",
    " High Thinking Fast",
    " XHigh Thinking Fast",
    " Max Thinking Fast",
    " No Thinking Fast",
    " Low Thinking",
    " Medium Thinking",
    " High Thinking",
    " XHigh Thinking",
    " Max Thinking",
    " No Thinking",
    " Low",
    " Medium",
    " High",
    " XHigh",
    " Max",
    " None",
    " Minimal",
    " Thinking",
    " Fast",
];
fn clean_display_name(name: &str) -> String {
    let mut trimmed = name.trim();
    loop {
        let lower = trimmed.to_lowercase();
        match DISPLAY_SUFFIXES
            .iter()
            .find(|suffix| lower.ends_with(&suffix.to_lowercase()))
        {
            Some(suffix) => trimmed = trimmed[..trimmed.len() - suffix.len()].trim(),
            None => return trimmed.into(),
        }
    }
}
fn level_order(level: &str) -> u8 {
    match level {
        "none" => 0,
        "minimal" => 1,
        "low" => 2,
        "medium" => 3,
        "high" => 4,
        "xhigh" => 5,
        "max" => 6,
        "fast" => 7,
        "priority" => 8,
        _ => 99,
    }
}
fn merge_unique(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}
fn aggregate(models: Vec<RegistryModelInfo>) -> Vec<RegistryModelInfo> {
    struct Entry {
        model: RegistryModelInfo,
        levels: HashSet<String>,
    }
    let mut entries: Vec<Entry> = Vec::with_capacity(models.len());
    for model in models {
        let clean = model.id.strip_prefix("devin/").unwrap_or(&model.id);
        let (mut base, effort) = split_devin_model_id(clean);
        if base.is_empty() {
            base = clean.into();
        }
        let is_base = base == clean;
        let id = format!("devin/{base}");
        let index = entries
            .iter()
            .position(|entry| entry.model.id == id)
            .unwrap_or_else(|| {
                let mut first = model.clone();
                first.id = id.clone();
                first.display_name = clean_display_name(&model.display_name);
                if first.display_name.is_empty() {
                    first.display_name = model.display_name.clone();
                }
                entries.push(Entry {
                    model: first,
                    levels: HashSet::new(),
                });
                entries.len() - 1
            });
        let entry = &mut entries[index];
        if is_base {
            if !model.display_name.is_empty() {
                entry.model.display_name = clean_display_name(&model.display_name);
            }
            if !model.owned_by.is_empty() {
                entry.model.owned_by = model.owned_by.clone();
            }
        }
        entry.model.context_length = entry.model.context_length.max(model.context_length);
        entry.model.max_completion_tokens = entry
            .model
            .max_completion_tokens
            .max(model.max_completion_tokens);
        entry.model.input_token_limit = entry.model.input_token_limit.max(model.input_token_limit);
        entry.model.output_token_limit =
            entry.model.output_token_limit.max(model.output_token_limit);
        merge_unique(
            &mut entry.model.supported_input_modalities,
            &model.supported_input_modalities,
        );
        merge_unique(
            &mut entry.model.supported_output_modalities,
            &model.supported_output_modalities,
        );
        merge_unique(
            &mut entry.model.supported_generation_methods,
            &model.supported_generation_methods,
        );
        if let Some(thinking) = model.thinking.as_ref() {
            entry.levels.extend(
                thinking
                    .levels
                    .iter()
                    .filter(|level| !level.is_empty() && *level != "priority")
                    .cloned(),
            );
        }
        if !effort.is_empty() && effort != "priority" {
            entry.levels.insert(effort);
        }
    }
    entries
        .into_iter()
        .map(|mut entry| {
            let model = &mut entry.model;
            if !entry.levels.is_empty() {
                let mut levels: Vec<_> = entry.levels.into_iter().collect();
                levels.sort_by(|left, right| {
                    level_order(left)
                        .cmp(&level_order(right))
                        .then_with(|| left.cmp(right))
                });
                model.thinking = Some(RegistryThinkingSupport {
                    levels,
                    ..RegistryThinkingSupport::default()
                });
            }
            if model.provider_type.is_empty() {
                model.provider_type = "devin".into();
            }
            if model.object.is_empty() {
                model.object = "model".into();
            }
            if model.supported_input_modalities.is_empty() {
                model.supported_input_modalities = vec!["text".into()];
            }
            if model.supported_output_modalities.is_empty() {
                model.supported_output_modalities = vec!["text".into()];
            }
            if model.input_token_limit == 0 && model.context_length > 0 {
                model.input_token_limit = model.context_length;
            }
            if model.output_token_limit == 0 && model.max_completion_tokens > 0 {
                model.output_token_limit = model.max_completion_tokens;
            }
            if model.supported_generation_methods.is_empty() {
                model.supported_generation_methods =
                    vec!["generateContent".into(), "countTokens".into()];
            }
            entry.model
        })
        .collect()
}
