// ref: internal/thinking/convert.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::ModelInfoView;
#[cfg(test)]
use crate::internal::registry::ModelInfo;

use super::{
    ThinkingLevel, LEVEL_AUTO, LEVEL_HIGH, LEVEL_LOW, LEVEL_MAX, LEVEL_MEDIUM, LEVEL_MINIMAL,
    LEVEL_NONE, LEVEL_XHIGH,
};

pub const THRESHOLD_MINIMAL: isize = 512;
pub const THRESHOLD_LOW: isize = 1024;
pub const THRESHOLD_MEDIUM: isize = 8192;
pub const THRESHOLD_HIGH: isize = 24_576;

/// Performs the canonical case-insensitive discrete-level to budget mapping.
pub fn convert_level_to_budget(level: &str) -> Option<isize> {
    if level.eq_ignore_ascii_case(LEVEL_NONE) {
        Some(0)
    } else if level.eq_ignore_ascii_case(LEVEL_AUTO) {
        Some(-1)
    } else if level.eq_ignore_ascii_case(LEVEL_MINIMAL) {
        Some(512)
    } else if level.eq_ignore_ascii_case(LEVEL_LOW) {
        Some(1024)
    } else if level.eq_ignore_ascii_case(LEVEL_MEDIUM) {
        Some(8192)
    } else if level.eq_ignore_ascii_case(LEVEL_HIGH) {
        Some(24_576)
    } else if level.eq_ignore_ascii_case(LEVEL_XHIGH) {
        Some(32_768)
    } else if level.eq_ignore_ascii_case(LEVEL_MAX) {
        Some(128_000)
    } else {
        None
    }
}

/// Maps a native-width token budget to the canonical discrete level.
pub fn convert_budget_to_level(budget: isize) -> Option<ThinkingLevel> {
    let level = match budget {
        value if value < -1 => return None,
        -1 => LEVEL_AUTO,
        0 => LEVEL_NONE,
        value if value <= THRESHOLD_MINIMAL => LEVEL_MINIMAL,
        value if value <= THRESHOLD_LOW => LEVEL_LOW,
        value if value <= THRESHOLD_MEDIUM => LEVEL_MEDIUM,
        value if value <= THRESHOLD_HIGH => LEVEL_HIGH,
        _ => LEVEL_XHIGH,
    };
    Some(ThinkingLevel::new(level))
}

/// Reports whether a level occurs case-insensitively after trimming entries.
pub fn has_level(levels: &[&str], target: &str) -> bool {
    levels
        .iter()
        .any(|level| level.trim().eq_ignore_ascii_case(target))
}

/// Maps a generic level to Claude's adaptive effort vocabulary.
pub fn map_to_claude_effort(level: &str, supports_max: bool) -> Option<&'static str> {
    let level = level.trim();
    if level.eq_ignore_ascii_case(LEVEL_MINIMAL) || level.eq_ignore_ascii_case(LEVEL_LOW) {
        Some(LEVEL_LOW)
    } else if level.eq_ignore_ascii_case(LEVEL_MEDIUM) {
        Some(LEVEL_MEDIUM)
    } else if level.eq_ignore_ascii_case(LEVEL_HIGH) {
        Some(LEVEL_HIGH)
    } else if level.eq_ignore_ascii_case(LEVEL_XHIGH) || level.eq_ignore_ascii_case(LEVEL_MAX) {
        Some(if supports_max { LEVEL_MAX } else { LEVEL_HIGH })
    } else if level.eq_ignore_ascii_case(LEVEL_AUTO) {
        Some(LEVEL_HIGH)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelCapability {
    Unknown,
    None,
    BudgetOnly,
    LevelOnly,
    Hybrid,
    Other(i32),
}

impl ModelCapability {
    pub const fn as_i32(self) -> i32 {
        match self {
            Self::Unknown => -1,
            Self::None => 0,
            Self::BudgetOnly => 1,
            Self::LevelOnly => 2,
            Self::Hybrid => 3,
            Self::Other(value) => value,
        }
    }
}

impl From<i32> for ModelCapability {
    fn from(value: i32) -> Self {
        match value {
            -1 => Self::Unknown,
            0 => Self::None,
            1 => Self::BudgetOnly,
            2 => Self::LevelOnly,
            3 => Self::Hybrid,
            value => Self::Other(value),
        }
    }
}

#[cfg(test)]
pub(super) fn detect_model_capability(model_info: Option<&ModelInfo>) -> ModelCapability {
    let view = model_info.map(ModelInfoView::from);
    detect_model_capability_view(view.as_ref())
}

pub(super) fn detect_model_capability_view(
    model_info: Option<&ModelInfoView<'_>>,
) -> ModelCapability {
    let Some(model_info) = model_info else {
        return ModelCapability::Unknown;
    };
    let Some(support) = model_info.thinking.as_ref() else {
        return ModelCapability::None;
    };
    let has_budget = support.min.unwrap_or_default() > 0 || support.max.unwrap_or_default() > 0;
    let has_levels = !support.levels.is_empty();
    match (has_budget, has_levels) {
        (true, true) => ModelCapability::Hybrid,
        (true, false) => ModelCapability::BudgetOnly,
        (false, true) => ModelCapability::LevelOnly,
        (false, false) => ModelCapability::None,
    }
}

/// Reports providers that can receive numeric thinking budgets.
pub fn is_budget_capable_provider(provider: &str) -> bool {
    matches!(provider, "gemini" | "antigravity" | "claude")
}
