// Origin: CTOX
// License: AGPL-3.0-only

mod apply;
mod convert;
mod errors;
mod json;
mod model_view;
mod provider;
mod strip;
mod suffix;
mod summary;
mod text;
mod types;
mod validate;

pub use convert::{
    convert_budget_to_level, convert_level_to_budget, has_level, is_budget_capable_provider,
    map_to_claude_effort, ModelCapability, THRESHOLD_HIGH, THRESHOLD_LOW, THRESHOLD_MEDIUM,
    THRESHOLD_MINIMAL,
};
pub use errors::{ErrorCode, ThinkingError};
pub use model_view::{ModelInfoView, ThinkingSupportView};
pub use provider::{
    AntigravityApplier, ClaudeApplier, CodexApplier, GeminiApplier, InteractionsApplier,
    KimiApplier, OpenAiApplier, XaiApplier,
};
pub use strip::strip_thinking_config;
pub use suffix::{parse_level_suffix, parse_numeric_suffix, parse_special_suffix, parse_suffix};
pub use summary::{
    apply_summary_config, apply_summary_config_for_model, apply_summary_config_for_resolved_model,
    extract_explicit_summary_config, extract_summary_config, extract_translated_summary_config,
    strip_inferred_claude_summary_activation, SummaryConfig, SummaryMode,
};
pub use text::get_thinking_text;
pub use types::{
    is_user_defined_model, ProviderApplier, SuffixResult, ThinkingConfig, ThinkingLevel,
    ThinkingMode, LEVEL_AUTO, LEVEL_HIGH, LEVEL_LOW, LEVEL_MAX, LEVEL_MEDIUM, LEVEL_MINIMAL,
    LEVEL_NONE, LEVEL_XHIGH,
};
pub use validate::validate_config;

#[cfg(test)]
mod summary_test;

#[cfg(test)]
mod core_test;

#[cfg(test)]
mod convert_validate_test;

#[cfg(test)]
mod strip_provider_test;

#[cfg(test)]
mod codex_gemini_test;

#[cfg(test)]
mod claude_antigravity_test;

#[cfg(test)]
mod kimi_interactions_test;

#[cfg(test)]
mod apply_configured_api_key_test;

#[cfg(test)]
mod kimi_max_clamp_repro_test;
pub use apply::{
    extract_reasoning_effort, extract_translated_reasoning_effort, EmbeddedModelInfoResolver,
    ModelInfoResolver, ResolvedCapabilityThinkingRequest, ResolvedThinkingRequest, ThinkingEngine,
    ThinkingRequest,
};

#[cfg(test)]
mod request_pipeline_candidate_test;
