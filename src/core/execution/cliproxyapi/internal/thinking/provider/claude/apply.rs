// ref: internal/thinking/provider/claude/apply.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use crate::internal::{
    registry::ModelInfo,
    thinking::ModelInfoView,
    thinking::{
        convert_level_to_budget,
        json::{get_path, remove_empty_object, remove_path, serialize_if_changed, set_path},
        model_view::is_user_defined_model_view as is_user_defined_model,
        ProviderApplier, ThinkingConfig, ThinkingError, ThinkingMode,
    },
};

const THINKING_TYPE: &str = "thinking.type";
const THINKING_BUDGET: &str = "thinking.budget_tokens";
const THINKING_DISPLAY: &str = "thinking.display";
const OUTPUT_CONFIG: &str = "output_config";
const OUTPUT_EFFORT: &str = "output_config.effort";
const MAX_TOKENS: &str = "max_tokens";

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
        match config.mode {
            ThinkingMode::None => Ok(apply_disabled(&normalized, true)),
            ThinkingMode::Auto => Ok(apply_auto(&normalized, false)),
            ThinkingMode::Level => {
                if config.level.is_empty() {
                    return Ok(normalized);
                }
                Ok(apply_adaptive(&normalized, Some(config.level.as_str())))
            }
            ThinkingMode::Budget => Ok(apply_manual(&normalized, config.budget)),
            ThinkingMode::Unknown(_) => unreachable!("unsupported modes returned above"),
        }
    }

    fn normalize_claude_budget(
        &self,
        body: &[u8],
        budget_tokens: isize,
        model_info: &ModelInfoView<'_>,
    ) -> Vec<u8> {
        if budget_tokens <= 0 {
            return body.to_vec();
        }
        let Ok(mut document) = serde_json::from_slice::<Value>(body) else {
            return body.to_vec();
        };
        let original = document.clone();
        let (effective_max, set_default_max) =
            effective_max_tokens(&document, model_info, MAX_TOKENS);
        if set_default_max && effective_max > 0 {
            set_signed(&mut document, MAX_TOKENS, effective_max);
        }

        let mut adjusted_budget = budget_tokens;
        if effective_max > 0 && adjusted_budget >= effective_max {
            adjusted_budget = effective_max - 1;
        }
        let min_budget = model_info
            .thinking
            .as_ref()
            .and_then(|support| support.min)
            .map(|value| value.clamp(isize::MIN as i128, isize::MAX as i128) as isize)
            .unwrap_or(0);
        if min_budget > 0 && adjusted_budget > 0 && adjusted_budget < min_budget {
            return serialize_if_changed(body, &original, &document);
        }
        if adjusted_budget != budget_tokens {
            set_signed(&mut document, THINKING_BUDGET, adjusted_budget);
        }
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
        let Some(model_info) = model_info else {
            return self.apply_compatible(body, config);
        };
        let Some(support) = model_info.thinking.as_ref() else {
            return Ok(body.to_vec());
        };
        let normalized = normalize_body(body);
        let supports_adaptive = !support.levels.is_empty();
        match config.mode {
            ThinkingMode::None => Ok(apply_disabled(&normalized, true)),
            ThinkingMode::Level if supports_adaptive && !config.level.is_empty() => {
                Ok(apply_adaptive(&normalized, Some(config.level.as_str())))
            }
            ThinkingMode::Level => {
                let Some(budget) = convert_level_to_budget(config.level.as_str()) else {
                    return Ok(normalized);
                };
                let manual = apply_manual(&normalized, budget);
                Ok(self.normalize_claude_budget(&manual, budget, model_info))
            }
            ThinkingMode::Budget if config.budget == 0 => Ok(apply_disabled(&normalized, false)),
            ThinkingMode::Budget => {
                let manual = apply_manual(&normalized, config.budget);
                Ok(self.normalize_claude_budget(&manual, config.budget, model_info))
            }
            ThinkingMode::Auto => Ok(apply_auto(&normalized, supports_adaptive)),
            ThinkingMode::Unknown(_) => Ok(normalized),
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

fn apply_disabled(body: &[u8], remove_display: bool) -> Vec<u8> {
    mutate(body, |document| {
        set_path(document, THINKING_TYPE, Value::String("disabled".into()));
        remove_path(document, THINKING_BUDGET);
        if remove_display {
            remove_path(document, THINKING_DISPLAY);
        }
        remove_path(document, OUTPUT_EFFORT);
        remove_empty_object(document, OUTPUT_CONFIG);
    })
}

fn apply_adaptive(body: &[u8], effort: Option<&str>) -> Vec<u8> {
    mutate(body, |document| {
        set_path(document, THINKING_TYPE, Value::String("adaptive".into()));
        remove_path(document, THINKING_BUDGET);
        if let Some(effort) = effort {
            set_path(document, OUTPUT_EFFORT, Value::String(effort.to_owned()));
        } else {
            remove_path(document, OUTPUT_EFFORT);
            remove_empty_object(document, OUTPUT_CONFIG);
        }
    })
}

fn apply_manual(body: &[u8], budget: isize) -> Vec<u8> {
    mutate(body, |document| {
        set_path(document, THINKING_TYPE, Value::String("enabled".into()));
        set_signed(document, THINKING_BUDGET, budget);
        remove_path(document, OUTPUT_EFFORT);
        remove_empty_object(document, OUTPUT_CONFIG);
    })
}

fn apply_auto(body: &[u8], supports_adaptive: bool) -> Vec<u8> {
    mutate(body, |document| {
        set_path(
            document,
            THINKING_TYPE,
            Value::String(
                if supports_adaptive {
                    "adaptive"
                } else {
                    "enabled"
                }
                .into(),
            ),
        );
        remove_path(document, THINKING_BUDGET);
        remove_path(document, OUTPUT_EFFORT);
        remove_empty_object(document, OUTPUT_CONFIG);
    })
}

fn mutate(body: &[u8], operation: impl FnOnce(&mut Value)) -> Vec<u8> {
    let Ok(mut document) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let original = document.clone();
    operation(&mut document);
    serialize_if_changed(body, &original, &document)
}

pub(in crate::internal::thinking::provider) fn effective_max_tokens(
    document: &Value,
    model_info: &ModelInfoView<'_>,
    path: &str,
) -> (isize, bool) {
    if let Some(value) = get_path(document, path).and_then(json_integer) {
        if value > 0 {
            return (i64_to_isize(value), false);
        }
    }
    if model_info.max_completion_tokens > 0 {
        return (
            model_info.max_completion_tokens.min(isize::MAX as usize) as isize,
            true,
        );
    }
    (0, false)
}

fn json_integer(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| {
                number
                    .as_u64()
                    .map(|value| value.min(i64::MAX as u64) as i64)
            })
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

pub(in crate::internal::thinking::provider) fn set_signed(
    document: &mut Value,
    path: &str,
    value: isize,
) {
    set_path(document, path, Value::Number((value as i64).into()));
}

fn i64_to_isize(value: i64) -> isize {
    value.clamp(isize::MIN as i64, isize::MAX as i64) as isize
}
