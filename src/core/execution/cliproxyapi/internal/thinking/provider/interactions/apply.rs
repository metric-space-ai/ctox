// ref: internal/thinking/provider/interactions/apply.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use crate::internal::{
    registry::ModelInfo,
    thinking::ModelInfoView,
    thinking::{
        convert_budget_to_level,
        json::{get_path, remove_path, serialize_if_changed, set_path},
        ProviderApplier, ThinkingConfig, ThinkingError, ThinkingMode, LEVEL_AUTO, LEVEL_MAX,
        LEVEL_NONE, LEVEL_XHIGH,
    },
};

const THINKING_PATHS: &[&str] = &[
    "generation_config.thinking_level",
    "generation_config.thinkingLevel",
    "generation_config.thinking_budget",
    "generation_config.thinkingBudget",
    "generation_config.thinking_summaries",
    "generation_config.thinkingSummaries",
    "generation_config.thinking_config",
    "generation_config.thinkingConfig",
    "generationConfig.thinkingLevel",
    "generationConfig.thinking_level",
    "generationConfig.thinkingBudget",
    "generationConfig.thinking_budget",
    "generationConfig.thinkingSummaries",
    "generationConfig.thinking_summaries",
    "generationConfig.thinkingConfig",
];

#[derive(Clone, Copy, Debug, Default)]
pub struct Applier;

impl Applier {
    pub const fn new() -> Self {
        Self
    }
}

impl ProviderApplier for Applier {
    fn apply(
        &self,
        body: &[u8],
        config: &ThinkingConfig,
        model_info: Option<&ModelInfo>,
    ) -> Result<Vec<u8>, ThinkingError> {
        let view = model_info.map(ModelInfoView::from);
        self.apply_model_info(body, config, view.as_ref())
    }

    fn apply_model_info(
        &self,
        body: &[u8],
        config: &ThinkingConfig,
        model_info: Option<&ModelInfoView<'_>>,
    ) -> Result<Vec<u8>, ThinkingError> {
        if !matches!(
            config.mode,
            ThinkingMode::Budget | ThinkingMode::Level | ThinkingMode::None | ThinkingMode::Auto
        ) {
            return Ok(body.to_vec());
        }

        let normalized = normalize_body(body);
        let Ok(original) = serde_json::from_slice::<Value>(&normalized) else {
            return Ok(normalized);
        };
        let mut result = original.clone();
        for path in THINKING_PATHS {
            remove_path(&mut result, path);
        }

        match config.mode {
            ThinkingMode::Level => {
                apply_level(&mut result, &original, config.level.as_str(), model_info)
            }
            ThinkingMode::Budget => apply_budget(&mut result, &original, config.budget, model_info),
            ThinkingMode::Auto => restore_summaries(&mut result, &original),
            ThinkingMode::None if !config.level.is_empty() => {
                apply_level(&mut result, &original, config.level.as_str(), model_info)
            }
            ThinkingMode::None if config.budget > 0 => {
                apply_budget(&mut result, &original, config.budget, model_info)
            }
            ThinkingMode::None => {}
            ThinkingMode::Unknown(_) => unreachable!("unknown mode returned above"),
        }

        Ok(serialize_if_changed(&normalized, &original, &result))
    }
}

fn normalize_body(body: &[u8]) -> Vec<u8> {
    if !body.is_empty() && serde_json::from_slice::<Value>(body).is_ok() {
        body.to_vec()
    } else {
        b"{}".to_vec()
    }
}

fn apply_budget(
    result: &mut Value,
    original: &Value,
    budget: isize,
    model_info: Option<&ModelInfoView<'_>>,
) {
    let Some(level) = convert_budget_to_level(budget) else {
        restore_summaries(result, original);
        return;
    };
    if level.as_str() == LEVEL_NONE || level.as_str() == LEVEL_AUTO {
        restore_summaries(result, original);
    } else {
        apply_level(result, original, level.as_str(), model_info);
    }
}

fn apply_level(
    result: &mut Value,
    original: &Value,
    level: &str,
    model_info: Option<&ModelInfoView<'_>>,
) {
    let level = normalize_level(level, model_info);
    if !level.is_empty() {
        set_path(
            result,
            "generation_config.thinking_level",
            Value::String(level),
        );
    }
    restore_summaries(result, original);
}

fn restore_summaries(result: &mut Value, original: &Value) {
    for path in [
        "generation_config.thinking_summaries",
        "generation_config.thinkingSummaries",
    ] {
        let Some(value) = get_path(original, path).and_then(Value::as_str) else {
            continue;
        };
        let normalized = value.trim().to_ascii_lowercase();
        if normalized == "auto" || normalized == "none" {
            set_path(
                result,
                "generation_config.thinking_summaries",
                Value::String(normalized),
            );
            return;
        }
    }

    for path in [
        "generation_config.thinking_config.include_thoughts",
        "generation_config.thinking_config.includeThoughts",
        "generation_config.thinkingConfig.include_thoughts",
        "generation_config.thinkingConfig.includeThoughts",
    ] {
        let Some(include) = get_path(original, path).and_then(Value::as_bool) else {
            continue;
        };
        set_path(
            result,
            "generation_config.thinking_summaries",
            Value::String(if include { "auto" } else { "none" }.to_owned()),
        );
        return;
    }
}

fn normalize_level(level: &str, model_info: Option<&ModelInfoView<'_>>) -> String {
    let normalized = level.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == LEVEL_NONE || normalized == LEVEL_AUTO {
        return String::new();
    }
    if let Some(levels) = model_info
        .and_then(|model| model.thinking.as_ref())
        .map(|support| support.levels.as_ref())
        .filter(|levels| !levels.is_empty())
    {
        return levels
            .iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(&normalized))
            .or_else(|| levels.last())
            .map(|candidate| candidate.to_ascii_lowercase())
            .unwrap_or_default();
    }
    if normalized == LEVEL_MAX || normalized == LEVEL_XHIGH {
        "high".to_owned()
    } else {
        normalized
    }
}
