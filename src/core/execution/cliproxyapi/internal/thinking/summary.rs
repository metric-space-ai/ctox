// ref: internal/thinking/summary.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use crate::internal::registry::{
    lookup_model_info, static_model_definitions_by_channel, ModelInfo, ThinkingSupport,
};

use super::{
    json::{get_path, remove_empty_object, remove_path, serialize_if_changed, set_path},
    parse_suffix,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SummaryMode {
    #[default]
    Unspecified,
    Disabled,
    Enabled,
    Unknown(i32),
}

impl SummaryMode {
    pub const fn as_i32(self) -> i32 {
        match self {
            Self::Unspecified => 0,
            Self::Disabled => 1,
            Self::Enabled => 2,
            Self::Unknown(value) => value,
        }
    }
}

impl From<i32> for SummaryMode {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Unspecified,
            1 => Self::Disabled,
            2 => Self::Enabled,
            value => Self::Unknown(value),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SummaryConfig {
    pub mode: SummaryMode,
    pub detail: String,
}

impl SummaryConfig {
    fn enabled(detail: impl Into<String>) -> Self {
        Self {
            mode: SummaryMode::Enabled,
            detail: detail.into(),
        }
    }

    fn disabled() -> Self {
        Self {
            mode: SummaryMode::Disabled,
            detail: String::new(),
        }
    }
}

/// Reads protocol-specific reasoning-summary visibility intent.
pub fn extract_summary_config(body: &[u8], format: &str) -> SummaryConfig {
    let normalized = format.trim().to_ascii_lowercase();
    if !summary_format_supported(&normalized) || body.is_empty() {
        return SummaryConfig::default();
    }
    let Ok(document) = serde_json::from_slice::<Value>(body) else {
        return SummaryConfig::default();
    };

    match normalized.as_str() {
        "openai" => {
            if let Some(config) = extract_openai_explicit_summary_config(&document) {
                return config;
            }
            if let Some(effort) = get_path(&document, "reasoning_effort").and_then(Value::as_str) {
                let effort = effort.trim().to_ascii_lowercase();
                if effort.is_empty() {
                    return SummaryConfig::default();
                }
                if effort == "none" {
                    return SummaryConfig::disabled();
                }
                return SummaryConfig::enabled("auto");
            }
        }
        "openai-response" | "codex" => {
            for path in ["reasoning.summary", "reasoning.generate_summary"] {
                if let Some(config) = responses_summary_config(&document, path) {
                    return config;
                }
            }
        }
        "claude" => {
            if !claude_thinking_accepts_display(&document) {
                return SummaryConfig::default();
            }
            if let Some(config) = claude_summary_config(&document, "thinking.display") {
                return config;
            }
        }
        "gemini" => {
            if let Some(config) = first_summary_bool_config(
                &document,
                &[
                    "generationConfig.thinkingConfig.includeThoughts",
                    "generationConfig.thinkingConfig.include_thoughts",
                    "generation_config.thinking_config.include_thoughts",
                    "generation_config.thinking_config.includeThoughts",
                ],
            ) {
                return config;
            }
        }
        "antigravity" => {
            if let Some(config) = first_summary_bool_config(
                &document,
                &[
                    "request.generationConfig.thinkingConfig.includeThoughts",
                    "request.generationConfig.thinkingConfig.include_thoughts",
                    "request.generationConfig.thinking_config.includeThoughts",
                    "request.generationConfig.thinking_config.include_thoughts",
                ],
            ) {
                return config;
            }
        }
        "interactions" => {
            for path in [
                "generation_config.thinking_summaries",
                "generation_config.thinkingSummaries",
            ] {
                if let Some(config) = interactions_summary_config(&document, path) {
                    return config;
                }
            }
            if let Some(config) = interactions_summary_config(&document, "reasoning.summary") {
                return config;
            }
            if let Some(config) = first_summary_bool_config(
                &document,
                &[
                    "generation_config.thinking_config.include_thoughts",
                    "generation_config.thinking_config.includeThoughts",
                    "generation_config.thinkingConfig.include_thoughts",
                    "generation_config.thinkingConfig.includeThoughts",
                ],
            ) {
                return config;
            }
        }
        _ => {}
    }

    SummaryConfig::default()
}

/// Reads only explicit visibility controls. OpenAI Chat effort is not used as
/// a visibility proxy in this variant.
pub fn extract_explicit_summary_config(body: &[u8], format: &str) -> SummaryConfig {
    let normalized = format.trim().to_ascii_lowercase();
    if normalized != "openai" {
        return extract_summary_config(body, &normalized);
    }
    let Ok(document) = serde_json::from_slice::<Value>(body) else {
        return SummaryConfig::default();
    };
    extract_openai_explicit_summary_config(&document).unwrap_or_default()
}

/// ref: internal/thinking/summary.go:133-144 @ e2bff010
/// Chat reasoning effort controls depth, not Claude display visibility.
pub fn extract_translated_summary_config(
    body: &[u8],
    source_format: &str,
    target_format: &str,
) -> SummaryConfig {
    if source_format.trim().eq_ignore_ascii_case("openai")
        && target_format.trim().eq_ignore_ascii_case("claude")
    {
        extract_explicit_summary_config(body, source_format)
    } else {
        extract_summary_config(body, source_format)
    }
}

pub fn apply_summary_config(body: &[u8], format: &str, config: &SummaryConfig) -> Vec<u8> {
    apply_summary_config_for_model(body, format, "", config)
}

pub fn apply_summary_config_for_model(
    body: &[u8],
    format: &str,
    model: &str,
    config: &SummaryConfig,
) -> Vec<u8> {
    apply_summary_config_for_provider(body, format, model, "", None, config)
}

pub fn apply_summary_config_for_resolved_model(
    body: &[u8],
    format: &str,
    model: &str,
    model_info: Option<&ModelInfo>,
    config: &SummaryConfig,
) -> Vec<u8> {
    apply_summary_config_for_provider(body, format, model, "", model_info, config)
}

pub(crate) fn apply_summary_config_for_provider(
    body: &[u8],
    format: &str,
    model: &str,
    provider: &str,
    model_info: Option<&ModelInfo>,
    config: &SummaryConfig,
) -> Vec<u8> {
    let normalized = format.trim().to_ascii_lowercase();
    if config.mode == SummaryMode::Unspecified
        || !summary_format_supported(&normalized)
        || body.is_empty()
    {
        return body.to_vec();
    }
    let Ok(mut document) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let original_document = document.clone();
    let enabled = config.mode == SummaryMode::Enabled;

    match normalized.as_str() {
        "openai" => apply_openai_chat_summary_config(&mut document, provider, enabled),
        "claude" => {
            if enabled && get_path(&document, "thinking.type").is_none() {
                enable_claude_thinking_for_summary(&mut document, model, model_info);
            }
            if !claude_thinking_accepts_display(&document) {
                return body.to_vec();
            }
            set_path(
                &mut document,
                "thinking.display",
                Value::String(if enabled { "summarized" } else { "omitted" }.into()),
            );
        }
        "gemini" => {
            set_path(
                &mut document,
                "generationConfig.thinkingConfig.includeThoughts",
                Value::Bool(enabled),
            );
            for path in [
                "generationConfig.thinkingConfig.include_thoughts",
                "generation_config.thinking_config.include_thoughts",
                "generation_config.thinking_config.includeThoughts",
            ] {
                remove_path(&mut document, path);
            }
        }
        "antigravity" => {
            set_path(
                &mut document,
                "request.generationConfig.thinkingConfig.includeThoughts",
                Value::Bool(enabled),
            );
            for path in [
                "request.generationConfig.thinkingConfig.include_thoughts",
                "request.generationConfig.thinking_config.include_thoughts",
                "request.generationConfig.thinking_config.includeThoughts",
            ] {
                remove_path(&mut document, path);
            }
        }
        "interactions" => {
            set_path(
                &mut document,
                "generation_config.thinking_summaries",
                Value::String(if enabled { "auto" } else { "none" }.into()),
            );
            remove_path(&mut document, "generation_config.thinkingSummaries");
        }
        "openai-response" | "codex" => {
            if enabled {
                set_path(
                    &mut document,
                    "reasoning.summary",
                    Value::String(normalized_summary_detail(&config.detail).into()),
                );
                remove_path(&mut document, "reasoning.generate_summary");
            } else {
                remove_path(&mut document, "reasoning.summary");
                remove_path(&mut document, "reasoning.generate_summary");
                remove_empty_object(&mut document, "reasoning");
            }
        }
        _ => unreachable!("supported formats are exhausted above"),
    }

    serialize_if_changed(body, &original_document, &document)
}

fn summary_format_supported(format: &str) -> bool {
    matches!(
        format,
        "openai"
            | "openai-response"
            | "codex"
            | "claude"
            | "gemini"
            | "antigravity"
            | "interactions"
    )
}

fn claude_thinking_accepts_display(document: &Value) -> bool {
    let thinking_type = get_path(document, "thinking.type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match thinking_type.as_str() {
        "adaptive" => true,
        "enabled" => match get_path(document, "thinking.budget_tokens") {
            Some(Value::Number(number)) => {
                let budget = json_number_to_i64(number);
                budget == -1 || budget > 0
            }
            _ => true,
        },
        _ => false,
    }
}

fn apply_openai_chat_summary_config(document: &mut Value, provider: &str, enabled: bool) {
    if is_openrouter_provider(provider)
        || get_path(document, "reasoning.exclude").is_some_and(Value::is_boolean)
    {
        set_path(document, "reasoning.exclude", Value::Bool(!enabled));
    }
    if get_path(document, "include_reasoning").is_some_and(Value::is_boolean) {
        set_path(document, "include_reasoning", Value::Bool(enabled));
    }
}

fn is_openrouter_provider(provider: &str) -> bool {
    let provider = provider.trim().to_ascii_lowercase();
    provider == "openrouter"
        || provider
            .split(['-', '_', '/', '.', ':'])
            .any(|part| part == "openrouter")
}

fn extract_openai_explicit_summary_config(document: &Value) -> Option<SummaryConfig> {
    for path in [
        "extra_body.google.thinking_config.include_thoughts",
        "extra_body.google.thinking_config.includeThoughts",
        "extra_body.google.thinkingConfig.include_thoughts",
        "extra_body.google.thinkingConfig.includeThoughts",
        "extra_body.extra_body.google.thinking_config.include_thoughts",
        "extra_body.extra_body.google.thinking_config.includeThoughts",
        "google.thinking_config.include_thoughts",
        "google.thinking_config.includeThoughts",
        "thinking.includeThoughts",
        "thinking.include_thoughts",
        "reasoning.includeThoughts",
        "reasoning.include_thoughts",
        "generationConfig.thinkingConfig.includeThoughts",
        "generationConfig.thinkingConfig.include_thoughts",
        "generation_config.thinking_config.include_thoughts",
        "generation_config.thinking_config.includeThoughts",
    ] {
        if let Some(config) = summary_bool_config(document, path) {
            return Some(config);
        }
    }
    for path in ["reasoning.summary", "reasoning.generate_summary"] {
        if let Some(config) = responses_summary_config(document, path) {
            return Some(config);
        }
    }
    if let Some(exclude) = get_path(document, "reasoning.exclude").and_then(Value::as_bool) {
        return Some(if exclude {
            SummaryConfig::disabled()
        } else {
            SummaryConfig::enabled("auto")
        });
    }
    if let Some(include) = get_path(document, "include_reasoning").and_then(Value::as_bool) {
        return Some(if include {
            SummaryConfig::enabled("auto")
        } else {
            SummaryConfig::disabled()
        });
    }
    get_path(document, "reasoning.enabled")
        .and_then(Value::as_bool)
        .map(|enabled| {
            if enabled {
                SummaryConfig::enabled("auto")
            } else {
                SummaryConfig::disabled()
            }
        })
}

fn first_summary_bool_config(document: &Value, paths: &[&str]) -> Option<SummaryConfig> {
    paths
        .iter()
        .find_map(|path| summary_bool_config(document, path))
}

fn summary_bool_config(document: &Value, path: &str) -> Option<SummaryConfig> {
    get_path(document, path)
        .and_then(Value::as_bool)
        .map(|enabled| {
            if enabled {
                SummaryConfig::enabled("auto")
            } else {
                SummaryConfig::disabled()
            }
        })
}

fn responses_summary_config(document: &Value, path: &str) -> Option<SummaryConfig> {
    let value = get_path(document, path)?;
    if value.is_null() {
        return Some(SummaryConfig::disabled());
    }
    match value.as_str()?.trim().to_ascii_lowercase().as_str() {
        detail @ ("auto" | "concise" | "detailed") => Some(SummaryConfig::enabled(detail)),
        "none" => Some(SummaryConfig::disabled()),
        _ => None,
    }
}

fn claude_summary_config(document: &Value, path: &str) -> Option<SummaryConfig> {
    match get_path(document, path)?
        .as_str()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "summarized" => Some(SummaryConfig::enabled("auto")),
        "omitted" => Some(SummaryConfig::disabled()),
        _ => None,
    }
}

fn interactions_summary_config(document: &Value, path: &str) -> Option<SummaryConfig> {
    match get_path(document, path)?
        .as_str()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "auto" => Some(SummaryConfig::enabled("auto")),
        "none" => Some(SummaryConfig::disabled()),
        _ => None,
    }
}

pub fn strip_inferred_claude_summary_activation(
    body: &[u8],
    model_info: Option<&ModelInfo>,
) -> Vec<u8> {
    let Some(support) = model_info.and_then(|model| model.thinking.as_ref()) else {
        return body.to_vec();
    };
    if !support.levels.is_empty() || support.min.unwrap_or_default() == 0 {
        return body.to_vec();
    }
    let Ok(mut document) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    if !get_path(&document, "thinking.type")
        .and_then(Value::as_str)
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("adaptive"))
    {
        return body.to_vec();
    }
    for path in [
        "thinking.type",
        "thinking.budget_tokens",
        "thinking.display",
        "output_config.effort",
    ] {
        remove_path(&mut document, path);
    }
    remove_empty_object(&mut document, "thinking");
    remove_empty_object(&mut document, "output_config");
    serde_json::to_vec(&document).unwrap_or_else(|_| body.to_vec())
}

fn enable_claude_thinking_for_summary(
    document: &mut Value,
    model: &str,
    resolved_model_info: Option<&ModelInfo>,
) {
    let body_model = get_path(document, "model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let base_model = {
        let requested = parse_suffix(model).model_name;
        if requested.is_empty() {
            parse_suffix(body_model).model_name
        } else {
            requested
        }
    };

    let owned_model_info = if resolved_model_info.is_none() {
        lookup_model_info(&base_model, "claude")
    } else {
        None
    };
    if let Some(support) = resolved_model_info
        .or(owned_model_info.as_ref())
        .and_then(|info| info.thinking.as_ref())
    {
        enable_claude_with_support(document, support);
        return;
    }

    // The active Rust registry is still a capability subset, while its pinned
    // static catalog is complete. Preserve the upstream public lookup behavior
    // for known catalog models until model_registry.go's complete port replaces
    // this compatibility fallback.
    let Some(catalog) = static_model_definitions_by_channel("claude").ok().flatten() else {
        return;
    };
    let Some(model) = catalog.as_array().and_then(|models| {
        models.iter().find(|candidate| {
            candidate
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.eq_ignore_ascii_case(&base_model))
        })
    }) else {
        return;
    };
    let Some(thinking) = model.get("thinking") else {
        return;
    };
    let has_levels = thinking
        .get("levels")
        .and_then(Value::as_array)
        .is_some_and(|levels| !levels.is_empty());
    if has_levels {
        set_path(document, "thinking.type", Value::String("adaptive".into()));
        remove_path(document, "thinking.budget_tokens");
        return;
    }
    let Some(budget) = thinking.get("min").and_then(Value::as_u64) else {
        return;
    };
    enable_claude_with_budget(document, budget);
}

fn enable_claude_with_support(document: &mut Value, support: &ThinkingSupport) {
    if !support.levels.is_empty() {
        set_path(document, "thinking.type", Value::String("adaptive".into()));
        remove_path(document, "thinking.budget_tokens");
        return;
    }
    let Some(budget) = support.min.filter(|budget| *budget > 0) else {
        return;
    };
    enable_claude_with_budget(document, budget);
}

fn enable_claude_with_budget(document: &mut Value, budget: u64) {
    if let Some(max_tokens) = get_path(document, "max_tokens") {
        let max_tokens = match max_tokens {
            Value::Number(number) => json_number_to_i64(number),
            Value::String(value) => value.parse::<i64>().unwrap_or_default(),
            _ => 0,
        };
        if max_tokens <= i64::try_from(budget).unwrap_or(i64::MAX) {
            return;
        }
    }
    set_path(document, "thinking.type", Value::String("enabled".into()));
    set_path(
        document,
        "thinking.budget_tokens",
        Value::Number(budget.into()),
    );
}

fn json_number_to_i64(number: &serde_json::Number) -> i64 {
    number
        .as_i64()
        .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| number.as_f64().map(|value| value as i64))
        .unwrap_or_default()
}

fn normalized_summary_detail(detail: &str) -> &'static str {
    if detail.trim().eq_ignore_ascii_case("concise") {
        "concise"
    } else if detail.trim().eq_ignore_ascii_case("detailed") {
        "detailed"
    } else {
        "auto"
    }
}
