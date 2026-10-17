// ref: internal/thinking/apply_configured_api_key_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use serde_json::Value;

use crate::internal::registry::{ModelInfo, ThinkingSupport};

use super::*;

const HIGH: &[&str] = &["high"];
const HIGH_MAX: &[&str] = &["high", "max"];
const HIGH_XHIGH: &[&str] = &["high", "xhigh"];
const HIGH_MAX_XHIGH: &[&str] = &["high", "max", "xhigh"];
const HIGH_XHIGH_MAX: &[&str] = &["high", "xhigh", "max"];
const OPENAI_LEVELS: &[&str] = &["low", "medium", "high"];

fn model(
    id: &'static str,
    provider_type: &'static str,
    levels: &'static [&'static str],
) -> ModelInfo {
    ModelInfo {
        id,
        provider_type,
        user_defined: false,
        max_completion_tokens: 0,
        thinking: Some(ThinkingSupport {
            min: None,
            max: None,
            zero_allowed: false,
            dynamic_allowed: false,
            levels,
        }),
    }
}

fn path(body: &[u8], candidate: &str) -> Option<Value> {
    let document = serde_json::from_slice::<Value>(body).ok()?;
    candidate
        .split('.')
        .try_fold(&document, |value, segment| value.get(segment))
        .cloned()
}

fn resolved<'a>(
    body: &'a [u8],
    source_body: &'a [u8],
    model: &'a str,
    from_format: &'a str,
    to_format: &'a str,
    provider_key: &'a str,
    model_info: Option<&'a ModelInfo>,
) -> ResolvedThinkingRequest<'a> {
    ResolvedThinkingRequest {
        body,
        source_body,
        model,
        from_format,
        to_format,
        provider_key,
        model_info,
    }
}

#[test]
fn maps_cross_family_high_intent() {
    for (source, supported, expected) in [
        ("xhigh", HIGH_MAX_XHIGH, "xhigh"),
        ("xhigh", HIGH_MAX, "max"),
        ("xhigh", HIGH, "high"),
        ("max", HIGH_XHIGH_MAX, "max"),
        ("max", HIGH_XHIGH, "xhigh"),
        ("max", HIGH, "high"),
    ] {
        let info = model("claude-upstream", "claude", supported);
        let source = format!(r#"{{"reasoning_effort":"{source}"}}"#);
        let output = ThinkingEngine::default()
            .apply_thinking_with_model_info(resolved(
                br#"{"thinking":{"type":"adaptive"},"output_config":{"effort":"low"}}"#,
                source.as_bytes(),
                "claude-upstream",
                "openai",
                "claude",
                "claude",
                Some(&info),
            ))
            .unwrap();
        assert_eq!(
            path(&output, "output_config.effort"),
            Some(Value::String(expected.into()))
        );
    }
}

#[test]
fn maps_openai_compatibility_high_intent() {
    let info = model("compat-upstream", "openai-compatibility", HIGH_MAX);
    let output = ThinkingEngine::default()
        .apply_thinking_with_model_info(resolved(
            br#"{"reasoning_effort":"high"}"#,
            br#"{"reasoning_effort":"xhigh"}"#,
            "compat-upstream",
            "openai",
            "openai",
            "compat-provider",
            Some(&info),
        ))
        .unwrap();
    assert_eq!(
        path(&output, "reasoning_effort"),
        Some(Value::String("max".into()))
    );
}

#[test]
fn maps_responses_to_codex_high_intent() {
    let info = model("codex-upstream", "codex", HIGH_XHIGH);
    let output = ThinkingEngine::default()
        .apply_thinking_with_model_info(resolved(
            br#"{"reasoning":{"effort":"high"}}"#,
            br#"{"reasoning":{"effort":"max"}}"#,
            "codex-upstream",
            "openai-response",
            "codex",
            "codex",
            Some(&info),
        ))
        .unwrap();
    assert_eq!(
        path(&output, "reasoning.effort"),
        Some(Value::String("xhigh".into()))
    );
}

#[test]
fn keeps_same_family_validation_strict() {
    let info = model("openai-upstream", "openai", OPENAI_LEVELS);
    let body = br#"{"reasoning_effort":"xhigh"}"#;
    let result = ThinkingEngine::default().apply_thinking_with_model_info(resolved(
        body,
        body,
        "openai-upstream",
        "openai",
        "openai",
        "openai",
        Some(&info),
    ));
    assert!(result.is_err());
}

#[test]
fn applies_enabled_summary_only_claude_visibility() {
    let info = model("private-claude", "claude", HIGH);
    let output = ThinkingEngine::default()
        .apply_thinking_with_model_info(resolved(
            br#"{"model":"private-claude","max_tokens":32000}"#,
            br#"{"reasoning":{"summary":"auto"}}"#,
            "private-claude",
            "openai-response",
            "claude",
            "claude",
            Some(&info),
        ))
        .unwrap();
    assert_eq!(
        path(&output, "thinking.type"),
        Some(Value::String("adaptive".into()))
    );
    assert_eq!(
        path(&output, "thinking.display"),
        Some(Value::String("summarized".into()))
    );
}

#[test]
fn drops_inferred_claude_mode_when_summary_removed() {
    let info = ModelInfo {
        id: "private-manual-claude",
        provider_type: "claude",
        user_defined: false,
        max_completion_tokens: 0,
        thinking: Some(ThinkingSupport {
            min: Some(1024),
            max: Some(16_000),
            zero_allowed: false,
            dynamic_allowed: false,
            levels: &[],
        }),
    };
    let output = ThinkingEngine::default()
        .apply_thinking_with_model_info_and_summary(
            resolved(
                br#"{"model":"private-manual-claude","max_tokens":32000,"thinking":{"type":"adaptive"}}"#,
                br#"{"reasoning":{"summary":"auto"}}"#,
                "private-manual-claude",
                "openai-response",
                "claude",
                "claude",
                Some(&info),
            ),
            &SummaryConfig::default(),
        )
        .unwrap();
    assert_eq!(path(&output, "thinking"), None);
}

#[test]
fn disabled_summary_does_not_activate_claude() {
    let info = model("private-claude", "claude", HIGH);
    let output = ThinkingEngine::default()
        .apply_thinking_with_model_info(resolved(
            br#"{"model":"private-claude","max_tokens":32000}"#,
            br#"{"reasoning":{"summary":null}}"#,
            "private-claude",
            "openai-response",
            "claude",
            "claude",
            Some(&info),
        ))
        .unwrap();
    assert_eq!(path(&output, "thinking"), None);
}

#[test]
fn summary_only_does_not_invent_openai_effort() {
    let info = model("private-openai", "openai", HIGH_MAX);
    let output = ThinkingEngine::default()
        .apply_thinking_with_model_info(resolved(
            br#"{"model":"private-openai","messages":[{"role":"user","content":"hi"}]}"#,
            br#"{"model":"private-openai","reasoning":{"summary":"auto"},"input":"hi"}"#,
            "private-openai",
            "openai-response",
            "openai",
            "openai",
            Some(&info),
        ))
        .unwrap();
    assert_eq!(path(&output, "reasoning_effort"), None);
}

#[test]
fn openai_chat_suffix_none_survives_enabled_summary() {
    let output = ThinkingEngine::default()
        .apply_thinking_with_summary(
            ThinkingRequest {
                body: br#"{"model":"private-openai","messages":[{"role":"user","content":"hi"}]}"#,
                model: "private-openai(none)",
                from_format: "openai-response",
                to_format: "openai",
                provider_key: "openai",
            },
            &SummaryConfig {
                mode: SummaryMode::Enabled,
                detail: "auto".into(),
            },
        )
        .unwrap();
    assert_eq!(
        path(&output, "reasoning_effort"),
        Some(Value::String("none".into()))
    );
}

#[test]
fn uses_openrouter_visibility_without_inventing_effort() {
    let info = model("openrouter-model", "openai-compatibility", HIGH_MAX);
    let output = ThinkingEngine::default()
        .apply_thinking_with_model_info(resolved(
            br#"{"model":"openrouter-model","messages":[{"role":"user","content":"hi"}]}"#,
            br#"{"model":"openrouter-model","reasoning":{"summary":"auto"},"input":"hi"}"#,
            "openrouter-model",
            "openai-response",
            "openai",
            "openrouter",
            Some(&info),
        ))
        .unwrap();
    assert_eq!(path(&output, "reasoning.exclude"), Some(Value::Bool(false)));
    assert_eq!(path(&output, "reasoning_effort"), None);
}

#[test]
fn uses_original_responses_effort() {
    let info = model("claude-upstream", "claude", HIGH_MAX);
    let output = ThinkingEngine::default()
        .apply_thinking_with_model_info(resolved(
            br#"{"thinking":{"type":"adaptive"},"output_config":{"effort":"low"}}"#,
            br#"{"reasoning":{"effort":"xhigh"}}"#,
            "claude-upstream",
            "openai-response",
            "claude",
            "claude",
            Some(&info),
        ))
        .unwrap();
    assert_eq!(
        path(&output, "output_config.effort"),
        Some(Value::String("max".into()))
    );
}

struct MarkerApplier(&'static [u8]);

impl ProviderApplier for MarkerApplier {
    fn apply(
        &self,
        _body: &[u8],
        _config: &ThinkingConfig,
        _model_info: Option<&ModelInfo>,
    ) -> Result<Vec<u8>, ThinkingError> {
        Ok(self.0.to_vec())
    }
}
