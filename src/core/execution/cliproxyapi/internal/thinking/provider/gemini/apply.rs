// ref: internal/thinking/provider/gemini/apply.go @ a88197f845c979132c8978ea223c6af05cc81536
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

const THINKING_CONFIG: &str = "generationConfig.thinkingConfig";
const THINKING_BUDGET: &str = "generationConfig.thinkingConfig.thinkingBudget";
const THINKING_BUDGET_SNAKE: &str = "generationConfig.thinkingConfig.thinking_budget";
const THINKING_LEVEL: &str = "generationConfig.thinkingConfig.thinkingLevel";
const THINKING_LEVEL_SNAKE: &str = "generationConfig.thinkingConfig.thinking_level";
const INCLUDE_THOUGHTS: &str = "generationConfig.thinkingConfig.includeThoughts";
const INCLUDE_THOUGHTS_SNAKE: &str = "generationConfig.thinkingConfig.include_thoughts";

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
    ) -> Result<Vec<u8>, ThinkingError> {
        if !supported_mode(config.mode) {
            return Ok(body.to_vec());
        }
        let normalized = normalize_body(body);
        if config.mode == ThinkingMode::Auto {
            return Ok(self.apply_budget_format(&normalized, config));
        }
        if config.mode == ThinkingMode::Level
            || (config.mode == ThinkingMode::None && !config.level.is_empty())
        {
            return Ok(self.apply_level_format(&normalized, config));
        }
        Ok(self.apply_budget_format(&normalized, config))
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

    fn apply_budget_format(&self, body: &[u8], config: &ThinkingConfig) -> Vec<u8> {
        let Ok(mut document) = serde_json::from_slice::<Value>(body) else {
            return body.to_vec();
        };
        let original = document.clone();
        remove_path(&mut document, THINKING_LEVEL);
        remove_path(&mut document, THINKING_LEVEL_SNAKE);
        remove_path(&mut document, THINKING_BUDGET_SNAKE);
        remove_path(&mut document, INCLUDE_THOUGHTS);
        remove_path(&mut document, INCLUDE_THOUGHTS_SNAKE);
        set_path(
            &mut document,
            THINKING_BUDGET,
            Value::Number((config.budget as i64).into()),
        );
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
            return self.apply_compatible(body, config);
        }
        let Some(support) = model_info.and_then(|model| model.thinking.as_ref()) else {
            return Ok(body.to_vec());
        };
        if !supported_mode(config.mode) {
            return Ok(body.to_vec());
        }
        let normalized = normalize_body(body);
        match config.mode {
            ThinkingMode::Level => Ok(self.apply_level_format(&normalized, config)),
            ThinkingMode::None if !support.levels.is_empty() => {
                Ok(self.apply_level_format(&normalized, config))
            }
            ThinkingMode::Budget | ThinkingMode::None | ThinkingMode::Auto => {
                Ok(self.apply_budget_format(&normalized, config))
            }
            ThinkingMode::Unknown(_) => unreachable!("unsupported modes returned above"),
        }
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

fn restore_include_thoughts(document: &mut Value, original: &Value) {
    for path in [INCLUDE_THOUGHTS, INCLUDE_THOUGHTS_SNAKE] {
        if let Some(enabled) = get_path(original, path).and_then(Value::as_bool) {
            set_path(document, INCLUDE_THOUGHTS, Value::Bool(enabled));
            return;
        }
    }
}
