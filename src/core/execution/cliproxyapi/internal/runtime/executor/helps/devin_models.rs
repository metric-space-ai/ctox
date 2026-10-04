// ref: internal/runtime/executor/helps/devin_models.go:1-325
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — explicit host-owned catalog authority
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::registry::{DevinModelsStore, StaticModelsCatalog};
use crate::internal::thinking::parse_suffix;

// ref: devin_models.go:11-44 — direct variants retain their original bytes/case.
const KNOWN_DEVIN_SUFFIXES: &[&str] = &[
    "-none",
    "-low",
    "-medium",
    "-high",
    "-xhigh",
    "-max",
    "-fast",
    "-slow",
    "-priority",
    "-low-priority",
    "-medium-priority",
    "-high-priority",
    "-xhigh-priority",
    "-max-priority",
    "-low-fast",
    "-medium-fast",
    "-high-fast",
    "-xhigh-fast",
    "-max-fast",
    "-none-fast",
    "-thinking-1m",
    "-thinking",
    "-max-1m",
    "-none-1m",
    "_none",
    "_minimal",
    "_low",
    "_medium",
    "_high",
    "_xhigh",
    "_max",
    "_thinking",
];

pub fn has_devin_effort_suffix(model: &str) -> bool {
    let lower = model.trim().to_lowercase();
    KNOWN_DEVIN_SUFFIXES
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

// ref: devin_models.go:64-92
pub fn normalize_devin_thinking_level(level: &str, budget_tokens: i64) -> String {
    let normalized = level.trim().to_lowercase();
    match normalized.as_str() {
        "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "fast" => normalized,
        "none" | "off" | "disabled" => "none".into(),
        "auto" | "adaptive" => "high".into(),
        _ if budget_tokens > 0 => match budget_tokens {
            1..=4096 => "low",
            4097..=16384 => "medium",
            16385..=32768 => "high",
            _ => "max",
        }
        .into(),
        _ => String::new(),
    }
}

/// Resolves the exact Devin wire UID using the same catalog authority as host
/// model listings. The store is read at request time; no stale executor-global
/// catalog or caller-provided metadata becomes model authority.
// ref: devin_models.go:98-238
pub fn resolve_devin_chat_model_uid(
    raw_model: &str,
    thinking_level: &str,
    budget_tokens: i64,
    models: &DevinModelsStore,
    catalog: &StaticModelsCatalog,
) -> String {
    let model = raw_model.trim();
    if model.is_empty() {
        return "swe-2-high".into();
    }
    let clean_model = if model.to_lowercase().starts_with("devin/") {
        &model[6..]
    } else {
        model
    };
    if has_devin_effort_suffix(clean_model) {
        return clean_model.to_owned();
    }
    let parsed = parse_suffix(clean_model);
    let mut requested_level = thinking_level;
    let mut base_model = parsed.model_name.trim();
    if parsed.has_suffix {
        requested_level = &parsed.raw_suffix;
    } else if let Some(colon) = clean_model.rfind(':') {
        base_model = clean_model[..colon].trim();
        requested_level = clean_model[colon + 1..].trim();
    }
    let effort = normalize_devin_thinking_level(requested_level, budget_tokens);
    let lower_base = base_model.to_lowercase();
    let mut canonical_base = lower_base.replace('.', "-");
    match canonical_base.as_str() {
        "claude-haiku-4-5" => return "MODEL_PRIVATE_11".into(),
        "gpt-4-1" => return "MODEL_CHAT_GPT_4_1_2025_04_14".into(),
        _ => {}
    }
    if canonical_base == "claude-sonnet-4-5" || canonical_base.contains("sonnet-4-5") {
        return if !effort.is_empty() && effort != "none" {
            "MODEL_PRIVATE_3"
        } else {
            "MODEL_PRIVATE_2"
        }
        .into();
    }
    if canonical_base == "gemini-3-flash" {
        canonical_base = "gemini-3-8-flash".into();
    }
    let normalized_under = canonical_base.replace('-', "_");
    match normalized_under.as_str() {
        "model_gpt_5_2" => {
            let clamped = clamp_effort(&effort, &["none", "low", "medium", "high", "xhigh"], "low");
            return format!("MODEL_GPT_5_2_{}", clamped.to_uppercase());
        }
        "model_google_gemini_3_0_flash" => {
            let clamped = clamp_effort(&effort, &["minimal", "low", "medium", "high"], "high");
            return format!("MODEL_GOOGLE_GEMINI_3_0_FLASH_{}", clamped.to_uppercase());
        }
        "model_claude_4_5_opus" => {
            return if !effort.is_empty() && effort != "none" {
                "MODEL_CLAUDE_4_5_OPUS_THINKING"
            } else {
                "MODEL_CLAUDE_4_5_OPUS"
            }
            .into();
        }
        _ => {}
    }
    let info = models.lookup(&canonical_base, catalog).or_else(|| {
        (canonical_base != lower_base)
            .then(|| models.lookup(&lower_base, catalog))
            .flatten()
    });
    let allowed = info
        .as_ref()
        .and_then(|model| model.thinking.as_ref())
        .map(|thinking| thinking.levels.as_slice())
        .unwrap_or_default();
    match canonical_base.as_str() {
        "swe-1-7" => {
            return if effort == "medium" {
                "swe-1-7-medium"
            } else {
                "swe-1-7"
            }
            .into()
        }
        "swe-1-6" => {
            return if effort == "fast" {
                "swe-1-6-fast"
            } else {
                "swe-1-6"
            }
            .into()
        }
        "glm-5-2" => {
            return match effort.as_str() {
                "none" => "glm-5-2-none",
                "max" => "glm-5-2-max",
                _ => "glm-5-2",
            }
            .into()
        }
        "glm-5-2-1m" => {
            return match effort.as_str() {
                "none" => "glm-5-2-none-1m",
                "max" => "glm-5-2-max-1m",
                _ => "glm-5-2-1m",
            }
            .into()
        }
        "claude-opus-4-6" | "claude-sonnet-4-6" => {
            return if !effort.is_empty() && effort != "none" {
                format!("{canonical_base}-thinking")
            } else {
                canonical_base
            };
        }
        "claude-opus-4-6-1m" | "claude-sonnet-4-6-1m" => {
            return if !effort.is_empty() && effort != "none" {
                format!("{}-thinking-1m", canonical_base.trim_end_matches("-1m"))
            } else {
                canonical_base
            };
        }
        _ => {}
    }
    if allowed.is_empty() {
        return canonical_base;
    }
    let levels = allowed.iter().map(String::as_str).collect::<Vec<_>>();
    let default = select_default_devin_effort(&canonical_base, &levels);
    let clamped = clamp_effort(&effort, &levels, default);
    format!("{canonical_base}-{clamped}")
}

// ref: devin_models.go:240-282 — defaults inspect exact catalog level spelling.
fn select_default_devin_effort<'a>(base_model: &str, levels: &[&'a str]) -> &'a str {
    if base_model.contains("swe-2") {
        return "high";
    }
    let has = |level| levels.contains(&level);
    if has("none") && has("low") && base_model.starts_with("gpt-5") {
        return "low";
    }
    if has("high")
        && ["gemini", "grok", "glm", "deepseek", "kimi", "nemotron"]
            .iter()
            .any(|family| base_model.contains(family))
    {
        return "high";
    }
    for level in ["medium", "high", "low"] {
        if has(level) {
            return level;
        }
    }
    levels[0]
}

// ref: devin_models.go:284-325 — ties prefer the higher supported effort.
fn level_index(level: &str) -> Option<usize> {
    let lower = level.trim().to_lowercase();
    ["minimal", "low", "medium", "high", "xhigh", "max"]
        .iter()
        .position(|known| *known == lower)
}
fn clamp_effort<'a>(requested: &str, allowed: &[&'a str], default: &'a str) -> &'a str {
    if requested.is_empty() {
        return default;
    }
    let lower = requested.trim().to_lowercase();
    if let Some(exact) = allowed
        .iter()
        .find(|level| level.trim().to_lowercase() == lower)
    {
        return exact;
    }
    if lower == "none" {
        return default;
    }
    let Some(index) = level_index(&lower) else {
        return default;
    };
    let mut best = default;
    let mut best_distance = usize::MAX;
    let mut best_index = None;
    for allowed_level in allowed {
        let Some(allowed_index) = level_index(allowed_level) else {
            continue;
        };
        let distance = index.abs_diff(allowed_index);
        if distance < best_distance
            || (distance == best_distance && best_index.is_none_or(|best| allowed_index > best))
        {
            best = allowed_level;
            best_distance = distance;
            best_index = Some(allowed_index);
        }
    }
    best
}
