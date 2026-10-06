// ref: internal/thinking/provider/antigravity/apply.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use crate::internal::{
    registry::ModelInfo,
    thinking::ModelInfoView,
    thinking::{
        json::{get_path, remove_path, serialize_if_changed, set_path},
        model_view::is_user_defined_model_view as is_user_defined_model,
        ProviderApplier, ThinkingConfig, ThinkingError, ThinkingMode,
    },
};

use super::super::claude::apply::{effective_max_tokens, set_signed};

const THINKING_CONFIG: &str = "request.generationConfig.thinkingConfig";
const THINKING_BUDGET: &str = "request.generationConfig.thinkingConfig.thinkingBudget";
const THINKING_BUDGET_SNAKE: &str = "request.generationConfig.thinkingConfig.thinking_budget";
const THINKING_LEVEL: &str = "request.generationConfig.thinkingConfig.thinkingLevel";
const THINKING_LEVEL_SNAKE: &str = "request.generationConfig.thinkingConfig.thinking_level";
const INCLUDE_THOUGHTS: &str = "request.generationConfig.thinkingConfig.includeThoughts";
const INCLUDE_THOUGHTS_SNAKE: &str = "request.generationConfig.thinkingConfig.include_thoughts";
const MAX_OUTPUT_TOKENS: &str = "request.generationConfig.maxOutputTokens";

#[derive(Clone, Copy, Debug, Default)]
pub struct Applier;

impl Applier {
    pub const fn new() -> Self {
        Self
    }

    fn apply_compatible(
        &self,
        body: &[u8],
        config: &ThinkingConfig,
        model_info: Option<&ModelInfoView<'_>>,
    ) -> Result<Vec<u8>, ThinkingError> {
        if !supported_mode(config.mode) {
            return Ok(body.to_vec());
        }
        let normalized = normalize_body(body);
        let is_claude = model_info.is_some_and(is_claude_model);
        if config.mode == ThinkingMode::Auto {
            return Ok(self.apply_budget_format(&normalized, config, model_info, is_claude));
        }
        if config.mode == ThinkingMode::Level
            || (config.mode == ThinkingMode::None && !config.level.is_empty())
        {
            return Ok(self.apply_level_format(&normalized, config));
        }
        Ok(self.apply_budget_format(&normalized, config, model_info, is_claude))
    }

    fn apply_level_format(&self, body: &[u8], config: &ThinkingConfig) -> Vec<u8> {
        let Ok(mut document) = serde_json::from_slice::<Value>(body) else {
            return body.to_vec();
        };
        let original = document.clone();
        remove_path(&mut document, THINKING_BUDGET);
        remove_path(&mut document, THINKING_BUDGET_SNAKE);
        remove_path(&mut document, THINKING_LEVEL_SNAKE);
        remove_path(&mut document, INCLUDE_THOUGHTS);
        remove_path(&mut document, INCLUDE_THOUGHTS_SNAKE);

        if config.mode == ThinkingMode::None {
            if config.budget == 0 && config.level.is_empty() {
                remove_path(&mut document, THINKING_CONFIG);
                return serialize_if_changed(body, &original, &document);
            }
            if !config.level.is_empty() {
                set_path(
                    &mut document,
                    THINKING_LEVEL,
                    Value::String(config.level.as_str().to_owned()),
                );
            }
            restore_include_thoughts(&mut document, &original);
            return serialize_if_changed(body, &original, &document);
        }
        if config.mode != ThinkingMode::Level {
            return body.to_vec();
        }
        set_path(
            &mut document,
            THINKING_LEVEL,
            Value::String(config.level.as_str().to_owned()),
        );
        restore_include_thoughts(&mut document, &original);
        serialize_if_changed(body, &original, &document)
    }

    fn apply_budget_format(
        &self,
        body: &[u8],
        config: &ThinkingConfig,
        model_info: Option<&ModelInfoView<'_>>,
        is_claude: bool,
    ) -> Vec<u8> {
        let Ok(mut document) = serde_json::from_slice::<Value>(body) else {
            return body.to_vec();
        };
        let original = document.clone();
        remove_path(&mut document, THINKING_LEVEL);
        remove_path(&mut document, THINKING_LEVEL_SNAKE);
        remove_path(&mut document, THINKING_BUDGET_SNAKE);
        remove_path(&mut document, INCLUDE_THOUGHTS);
        remove_path(&mut document, INCLUDE_THOUGHTS_SNAKE);

        let mut budget = config.budget;
        if is_claude {
            if let Some(model_info) = model_info {
                let removed = normalize_claude_budget(&mut document, &mut budget, model_info);
                if removed {
                    restore_include_thoughts(&mut document, &original);
                    return serialize_if_changed(body, &original, &document);
                }
            }
        }
        set_signed(&mut document, THINKING_BUDGET, budget);
        restore_include_thoughts(&mut document, &original);
        serialize_if_changed(body, &original, &document)
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
        if is_user_defined_model(model_info) {
            return self.apply_compatible(body, config, model_info);
        }
        let Some(model_info) = model_info else {
            return self.apply_compatible(body, config, model_info);
        };
        let Some(support) = model_info.thinking.as_ref() else {
            return Ok(body.to_vec());
        };
        if !supported_mode(config.mode) {
            return Ok(body.to_vec());
        }
        let normalized = normalize_body(body);
        let is_claude = is_claude_model(model_info);
        if matches!(config.mode, ThinkingMode::Auto | ThinkingMode::Budget) {
            return Ok(self.apply_budget_format(&normalized, config, Some(model_info), is_claude));
        }
        if !support.levels.is_empty() {
            return Ok(self.apply_level_format(&normalized, config));
        }
        Ok(self.apply_budget_format(&normalized, config, Some(model_info), is_claude))
    }
}

fn supported_mode(mode: ThinkingMode) -> bool {
    matches!(
        mode,
        ThinkingMode::Budget | ThinkingMode::Level | ThinkingMode::None | ThinkingMode::Auto
    )
}

fn normalize_body(body: &[u8]) -> Vec<u8> {
    if !body.is_empty() && serde_json::from_slice::<Value>(body).is_ok() {
        body.to_vec()
    } else {
        b"{}".to_vec()
    }
}

fn is_claude_model(model_info: &ModelInfoView<'_>) -> bool {
    model_info.id.to_ascii_lowercase().contains("claude")
}

fn normalize_claude_budget(
    document: &mut Value,
    budget: &mut isize,
    model_info: &ModelInfoView<'_>,
) -> bool {
    let (effective_max, set_default_max) =
        effective_max_tokens(document, model_info, MAX_OUTPUT_TOKENS);
    if effective_max > 0 && *budget >= effective_max {
        *budget = effective_max - 1;
    }
    let min_budget = model_info
        .thinking
        .as_ref()
        .and_then(|support| support.min)
        .map(|value| value.clamp(isize::MIN as i128, isize::MAX as i128) as isize)
        .unwrap_or(0);
    if min_budget > 0 && *budget >= 0 && *budget < min_budget {
        remove_path(document, THINKING_CONFIG);
        return true;
    }
    if set_default_max && effective_max > 0 {
        set_signed(document, MAX_OUTPUT_TOKENS, effective_max);
    }
    false
}

fn restore_include_thoughts(document: &mut Value, original: &Value) {
    for path in [INCLUDE_THOUGHTS, INCLUDE_THOUGHTS_SNAKE] {
        if let Some(enabled) = get_path(original, path).and_then(Value::as_bool) {
            set_path(document, INCLUDE_THOUGHTS, Value::Bool(enabled));
            return;
        }
    }
}
