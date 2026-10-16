// ref: internal/modelconfig/model_info_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::model_hash::{
    compute_claude_models_hash, compute_codex_models_hash, compute_gemini_models_hash,
    compute_openai_compat_models_hash, compute_vertex_compat_models_hash, ModelAlias,
    OpenAiCompatibilityModel,
};
use super::model_info::{normalize_thinking_support, resolve_model_info, ThinkingSupport};

#[test]
fn resolve_model_info_uses_suffix_free_static_capabilities() {
    let info = resolve_model_info("claude-opus-4-6(high)", "claude", None);
    assert!(info.thinking.is_some(), "{info:?}");
    assert_eq!(info.id, "claude-opus-4-6(high)");
    assert!(!info.user_defined);
}

#[test]
fn resolve_model_info_explicit_thinking_overrides_and_clones() {
    let mut support = ThinkingSupport {
        levels: vec![" XHIGH ".into(), "xhigh".into(), " High ".into()],
        ..ThinkingSupport::default()
    };
    let info = resolve_model_info("custom-model", "codex", Some(&support));
    assert_eq!(info.thinking.as_ref().unwrap().levels, ["xhigh", "high"]);
    support.levels[0] = "low".into();
    assert_eq!(info.thinking.unwrap().levels[0], "xhigh");
}

#[test]
fn normalize_thinking_support_derives_special_level_flags() {
    let support = normalize_thinking_support(&ThinkingSupport {
        levels: vec!["low".into(), "none".into(), "auto".into()],
        ..ThinkingSupport::default()
    });
    assert!(support.zero_allowed);
    assert!(support.dynamic_allowed);
}

#[test]
fn resolve_model_info_unknown_model_keeps_missing_capability() {
    let info = resolve_model_info("unknown-configured-model", "claude", None);
    assert!(info.thinking.is_none(), "{:?}", info.thinking);
    assert!(!info.user_defined);
}
