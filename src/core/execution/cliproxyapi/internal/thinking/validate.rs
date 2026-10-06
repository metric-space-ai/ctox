// ref: internal/thinking/validate.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{ModelInfoView, ThinkingSupportView};
use crate::internal::registry::ModelInfo;

use super::{
    convert::detect_model_capability_view as detect_model_capability, convert_budget_to_level,
    convert_level_to_budget, ErrorCode, ModelCapability, ThinkingConfig, ThinkingError,
    ThinkingLevel, ThinkingMode, LEVEL_AUTO, LEVEL_NONE,
};

const STANDARD_LEVEL_ORDER: &[&str] = &["minimal", "low", "medium", "high", "xhigh", "max"];

/// Validates and normalizes a thinking configuration against model capability.
pub fn validate_config(
    config: ThinkingConfig,
    model_info: Option<&ModelInfo>,
    from_format: &str,
    to_format: &str,
    from_suffix: bool,
) -> Result<ThinkingConfig, ThinkingError> {
    let view = model_info.map(ModelInfoView::from);
    validate_config_view(config, view.as_ref(), from_format, to_format, from_suffix)
}

pub(super) fn validate_config_view(
    mut config: ThinkingConfig,
    model_info: Option<&ModelInfoView<'_>>,
    from_format: &str,
    to_format: &str,
    from_suffix: bool,
) -> Result<ThinkingConfig, ThinkingError> {
    let from_format = from_format.trim().to_ascii_lowercase();
    let to_format = to_format.trim().to_ascii_lowercase();
    let model = model_info
        .map(|info| info.id)
        .filter(|id| !id.is_empty())
        .unwrap_or("unknown");
    let support = model_info.and_then(|info| info.thinking.as_ref());

    let Some(support) = support else {
        if config.mode != ThinkingMode::None {
            return Err(ThinkingError::with_model(
                ErrorCode::ThinkingNotSupported,
                "thinking not supported for this model",
                model,
            ));
        }
        return Ok(config);
    };

    let capability = detect_model_capability(model_info);
    let target_has_level_support = matches!(
        capability,
        ModelCapability::LevelOnly | ModelCapability::Hybrid
    );
    let model_family_mismatch = model_info.is_some_and(|info| {
        let model_type = info.provider_type.trim().to_ascii_lowercase();
        !model_type.is_empty()
            && ((!from_format.is_empty() && !is_same_provider_family(&from_format, &model_type))
                || (!to_format.is_empty() && !is_same_provider_family(&to_format, &model_type)))
    });
    let allow_clamp_unsupported = target_has_level_support
        && (!is_same_provider_family(&from_format, &to_format) || model_family_mismatch);
    let strict_budget = !from_suffix
        && !from_format.is_empty()
        && is_same_provider_family(&from_format, &to_format)
        && !model_family_mismatch;
    let mut budget_derived_from_level = false;

    match capability {
        ModelCapability::BudgetOnly if config.mode == ThinkingMode::Level => {
            if config.level.as_str() != LEVEL_AUTO {
                let Some(budget) = convert_level_to_budget(config.level.as_str()) else {
                    return Err(ThinkingError::new(
                        ErrorCode::UnknownLevel,
                        format!("unknown level: {}", config.level),
                    ));
                };
                config.mode = ThinkingMode::Budget;
                config.budget = budget;
                config.level = ThinkingLevel::default();
                budget_derived_from_level = true;
            }
        }
        ModelCapability::LevelOnly if config.mode == ThinkingMode::Budget => {
            let Some(level) = convert_budget_to_level(config.budget) else {
                return Err(ThinkingError::new(
                    ErrorCode::UnknownLevel,
                    format!(
                        "budget {} cannot be converted to a valid level",
                        config.budget
                    ),
                ));
            };
            config.mode = ThinkingMode::Level;
            config.level = clamp_level(level, model_info);
            config.budget = 0;
        }
        _ => {}
    }

    if config.mode == ThinkingMode::Level && config.level.as_str() == LEVEL_NONE {
        config.mode = ThinkingMode::None;
        config.budget = 0;
        config.level = ThinkingLevel::default();
    }
    if config.mode == ThinkingMode::Level && config.level.as_str() == LEVEL_AUTO {
        config.mode = ThinkingMode::Auto;
        config.budget = -1;
        config.level = ThinkingLevel::default();
    }
    if config.mode == ThinkingMode::Budget && config.budget == 0 {
        config.mode = ThinkingMode::None;
        config.level = ThinkingLevel::default();
    }

    if !support.levels.is_empty()
        && config.mode == ThinkingMode::Level
        && !is_level_supported(config.level.as_str(), support.levels.as_ref())
    {
        if allow_clamp_unsupported {
            config.level = clamp_level(config.level, model_info);
        }
        if !is_level_supported(config.level.as_str(), support.levels.as_ref()) {
            let valid_levels = normalize_levels(support.levels.as_ref()).join(", ");
            let normalized_level = config.level.as_str().to_ascii_lowercase();
            return Err(ThinkingError::new(
                ErrorCode::LevelNotSupported,
                format!(
                    "level {} not supported, valid levels: {valid_levels}",
                    quote_go_string(&normalized_level)
                ),
            ));
        }
    }

    let (min, max) = support_bounds(support);
    if strict_budget
        && config.mode == ThinkingMode::Budget
        && !budget_derived_from_level
        && (min != 0 || max != 0)
        && (config.budget < min
            || config.budget > max
            || (config.budget == 0 && !support.zero_allowed))
    {
        return Err(ThinkingError::new(
            ErrorCode::BudgetOutOfRange,
            format!("budget {} out of range [{min},{max}]", config.budget),
        ));
    }

    if config.mode == ThinkingMode::Auto && !support.dynamic_allowed {
        config = convert_auto_to_mid_range(config, support);
        if config.mode == ThinkingMode::Level
            && !support.levels.is_empty()
            && !is_level_supported(config.level.as_str(), support.levels.as_ref())
        {
            config.level = clamp_level(config.level, model_info);
        }
    }

    if config.mode == ThinkingMode::None && to_format == "claude" {
        config.budget = 0;
        config.level = ThinkingLevel::default();
    } else {
        if matches!(
            config.mode,
            ThinkingMode::Budget | ThinkingMode::Auto | ThinkingMode::None
        ) {
            config.budget = clamp_budget(config.budget, model_info);
        }

        let cannot_disable_level_model =
            !support.zero_allowed && !is_level_supported(LEVEL_NONE, support.levels.as_ref());
        if config.mode == ThinkingMode::None
            && !support.levels.is_empty()
            && (config.budget > 0 || cannot_disable_level_model)
        {
            config.level = ThinkingLevel::new(support.levels[0]);
        }
    }

    Ok(config)
}

fn convert_auto_to_mid_range(
    mut config: ThinkingConfig,
    support: &ThinkingSupportView<'_>,
) -> ThinkingConfig {
    let (min, max) = support_bounds(support);
    if !support.levels.is_empty() && min == 0 && max == 0 {
        config.mode = ThinkingMode::Level;
        config.level = ThinkingLevel::new("medium");
        config.budget = 0;
        return config;
    }

    let middle = min.wrapping_add(max) / 2;
    if middle <= 0 && support.zero_allowed {
        config.mode = ThinkingMode::None;
        config.budget = 0;
    } else if middle <= 0 {
        config.mode = ThinkingMode::Budget;
        config.budget = min;
    } else {
        config.mode = ThinkingMode::Budget;
        config.budget = middle;
    }
    config
}

fn clamp_level(level: ThinkingLevel, model_info: Option<&ModelInfoView<'_>>) -> ThinkingLevel {
    let supported = model_info
        .and_then(|info| info.thinking.as_ref())
        .map(|support| support.levels.as_ref())
        .unwrap_or_default();
    if supported.is_empty() || is_level_supported(level.as_str(), supported) {
        return level;
    }
    let Some(position) = level_index(level.as_str()) else {
        return level;
    };

    let mut best: Option<(usize, usize)> = None;
    for supported_level in supported {
        let Some(index) = level_index(supported_level.trim()) else {
            continue;
        };
        let distance = position.abs_diff(index);
        if best.is_none_or(|(best_index, best_distance)| {
            distance < best_distance || (distance == best_distance && index < best_index)
        }) {
            best = Some((index, distance));
        }
    }
    best.map(|(index, _)| ThinkingLevel::new(STANDARD_LEVEL_ORDER[index]))
        .unwrap_or(level)
}

fn clamp_budget(value: isize, model_info: Option<&ModelInfoView<'_>>) -> isize {
    let Some(support) = model_info.and_then(|info| info.thinking.as_ref()) else {
        return value;
    };
    if value == -1 {
        return value;
    }
    let (min, max) = support_bounds(support);
    if value == 0 && !support.zero_allowed {
        return min;
    }
    if min == 0 && max == 0 {
        return value;
    }
    if value < min {
        if value == 0 && support.zero_allowed {
            0
        } else {
            min
        }
    } else if value > max {
        max
    } else {
        value
    }
}

fn support_bounds(support: &ThinkingSupportView<'_>) -> (isize, isize) {
    (
        support
            .min
            .map(|value| value.clamp(isize::MIN as i128, isize::MAX as i128) as isize)
            .unwrap_or_default(),
        support
            .max
            .map(|value| value.clamp(isize::MIN as i128, isize::MAX as i128) as isize)
            .unwrap_or_default(),
    )
}

fn is_level_supported(level: &str, supported: &[&str]) -> bool {
    supported
        .iter()
        .any(|candidate| level.eq_ignore_ascii_case(candidate.trim()))
}

fn level_index(level: &str) -> Option<usize> {
    STANDARD_LEVEL_ORDER
        .iter()
        .position(|candidate| level.eq_ignore_ascii_case(candidate))
}

fn normalize_levels(levels: &[&str]) -> Vec<String> {
    levels
        .iter()
        .map(|level| level.trim().to_ascii_lowercase())
        .collect()
}

fn is_gemini_family(provider: &str) -> bool {
    matches!(provider, "gemini" | "antigravity")
}

fn is_openai_family(provider: &str) -> bool {
    matches!(provider, "openai" | "openai-response" | "codex")
}

fn is_same_provider_family(from: &str, to: &str) -> bool {
    from == to
        || (is_gemini_family(from) && is_gemini_family(to))
        || (is_openai_family(from) && is_openai_family(to))
}

fn quote_go_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{value}\""))
}
