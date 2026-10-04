// Origin: CTOX
// License: AGPL-3.0-only

use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use super::{
    apply_summary_config_for_resolved_model, ErrorCode, ModelInfoResolver, ModelInfoView,
    ProviderApplier, SummaryConfig, SummaryMode, ThinkingConfig, ThinkingEngine, ThinkingError,
    ThinkingMode,
};
use crate::{
    internal::{
        modelconfig,
        registry::{ModelInfo, ThinkingSupport},
        runtime::executor::helps::{
            apply_request_thinking, RequestThinkingPipeline, RequestThinkingRoute,
        },
    },
    sdk::{
        cliproxy::executor::{Options, Request},
        translator::Registry,
    },
};

fn owned_model(id: &str, provider: &str, levels: &[&str]) -> modelconfig::ModelInfo {
    modelconfig::ModelInfo {
        id: id.to_owned(),
        provider_type: provider.to_owned(),
        thinking: Some(modelconfig::ThinkingSupport {
            levels: levels.iter().map(|level| (*level).to_owned()).collect(),
            zero_allowed: true,
            dynamic_allowed: true,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn pipeline(engine: ThinkingEngine) -> RequestThinkingPipeline {
    RequestThinkingPipeline::new(Arc::new(engine), Arc::new(Registry::new()))
}

#[allow(clippy::too_many_arguments)]
fn run(
    pipeline: &RequestThinkingPipeline,
    body: &[u8],
    current: &[u8],
    original: &[u8],
    model: &str,
    from: &str,
    to: &str,
    selected: Option<&modelconfig::ModelInfo>,
    static_info: Option<&ModelInfo>,
) -> Result<Vec<u8>, ThinkingError> {
    let request = Request {
        model: model.to_owned(),
        payload: current.to_vec(),
        ..Default::default()
    };
    let options = Options {
        original_request: original.to_vec(),
        ..Default::default()
    };
    apply_request_thinking(
        pipeline,
        body,
        &request,
        &options,
        RequestThinkingRoute {
            from_format: from,
            to_format: to,
            provider: to,
            resolved_config_model_info: selected,
            resolved_model_info: static_info,
        },
    )
}

fn at(bytes: &[u8], path: &str) -> Value {
    let value: Value = serde_json::from_slice(bytes).unwrap();
    path.split('.')
        .try_fold(&value, |current, key| current.get(key))
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn candidate_thinking_pipeline_owned_custom_level_overrides_static_capability() {
    let selected = owned_model("runtime-owned-gemini", "gemini", &["ultra-custom", "low"]);
    let conflicting_static = ModelInfo {
        id: "static-budget-model",
        provider_type: "gemini",
        user_defined: false,
        max_completion_tokens: 0,
        thinking: Some(ThinkingSupport {
            min: Some(100),
            max: Some(200),
            zero_allowed: false,
            dynamic_allowed: false,
            levels: &[],
        }),
    };
    let body = br#"{"generationConfig":{"thinkingConfig":{"thinkingLevel":"ultra-custom"}}}"#;
    let output = run(
        &pipeline(ThinkingEngine::default()),
        b"{}",
        body,
        b"{}",
        "runtime-owned-gemini",
        "gemini",
        "gemini",
        Some(&selected),
        Some(&conflicting_static),
    )
    .unwrap();
    assert_eq!(
        at(&output, "generationConfig.thinkingConfig.thinkingLevel"),
        json!("ultra-custom")
    );
    assert_eq!(
        at(&output, "generationConfig.thinkingConfig.thinkingBudget"),
        Value::Null
    );
}

#[test]
fn candidate_thinking_pipeline_owned_budget_bounds_and_dynamic_error_identity() {
    let mut selected = owned_model("runtime-private-budget", "gemini", &[]);
    selected.thinking.as_mut().unwrap().min = 200;
    selected.thinking.as_mut().unwrap().max = 400;
    let body = br#"{"generationConfig":{"thinkingConfig":{"thinkingBudget":900}}}"#;
    let error = run(
        &pipeline(ThinkingEngine::default()),
        b"{}",
        body,
        b"{}",
        &selected.id,
        "gemini",
        "gemini",
        Some(&selected),
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::BudgetOutOfRange);
    assert!(error.message.contains("900 out of range [200,400]"));
    selected.thinking = None;
    let view = ModelInfoView::from(&selected);
    let error = super::validate::validate_config_view(
        ThinkingConfig {
            mode: ThinkingMode::Budget,
            budget: 900,
            ..Default::default()
        },
        Some(&view),
        "gemini",
        "gemini",
        false,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ThinkingNotSupported);
    assert_eq!(error.model, "runtime-private-budget");
}

#[test]
fn candidate_thinking_pipeline_owned_capabilities_reach_all_native_appliers() {
    let pipeline = pipeline(ThinkingEngine::default());
    for (provider, path) in [
        ("gemini", "generationConfig.thinkingConfig.thinkingLevel"),
        ("interactions", "generation_config.thinking_level"),
        ("claude", "output_config.effort"),
        ("openai", "reasoning_effort"),
        ("codex", "reasoning.effort"),
        ("xai", "reasoning.effort"),
        ("kimi", "thinking.effort"),
        (
            "antigravity",
            "request.generationConfig.thinkingConfig.thinkingLevel",
        ),
    ] {
        let selected = owned_model(&format!("owned-{provider}"), provider, &["low", "high"]);
        let output = run(
            &pipeline,
            b"{}",
            b"{}",
            b"{}",
            &format!("{}(high)", selected.id),
            provider,
            provider,
            Some(&selected),
            None,
        )
        .unwrap();
        assert_eq!(at(&output, path), json!("high"), "provider {provider}");
    }
}

#[test]
fn candidate_thinking_pipeline_current_effort_and_original_summary_have_distinct_owners() {
    let selected = owned_model("runtime-google", "gemini", &["low", "high"]);
    let output = run(
        &pipeline(ThinkingEngine::default()),
        b"{}",
        br#"{"reasoning":{"effort":"high"}}"#,
        br#"{"reasoning":{"effort":"low","summary":"detailed"}}"#,
        &selected.id,
        "openai-response",
        "gemini",
        Some(&selected),
        None,
    )
    .unwrap();
    assert_eq!(
        at(&output, "generationConfig.thinkingConfig.thinkingLevel"),
        json!("high")
    );
    assert_eq!(
        at(&output, "generationConfig.thinkingConfig.includeThoughts"),
        json!(true)
    );
}

#[test]
fn candidate_thinking_pipeline_empty_current_falls_back_and_target_summary_wins() {
    let selected = owned_model("runtime-google", "gemini", &["low", "high"]);
    let output = run(
        &pipeline(ThinkingEngine::default()),
        br#"{"generationConfig":{"thinkingConfig":{"includeThoughts":false}}}"#,
        b"",
        br#"{"reasoning":{"effort":"high","summary":"detailed"}}"#,
        &selected.id,
        "openai-response",
        "gemini",
        Some(&selected),
        None,
    )
    .unwrap();
    assert_eq!(
        at(&output, "generationConfig.thinkingConfig.thinkingLevel"),
        json!("high")
    );
    assert_eq!(
        at(&output, "generationConfig.thinkingConfig.includeThoughts"),
        json!(false)
    );
}

#[test]
fn candidate_thinking_pipeline_owned_claude_budget_preserves_token_ceiling_and_summary() {
    let mut selected = owned_model("runtime-claude-budget", "claude", &[]);
    selected.thinking.as_mut().unwrap().min = 1024;
    selected.thinking.as_mut().unwrap().max = 8192;
    selected.max_completion_tokens = 4096;
    let source = br#"{"reasoning":{"effort":"medium","summary":"detailed"}}"#;
    let current = br#"{"reasoning":{"effort":"medium"}}"#;
    let output = run(
        &pipeline(ThinkingEngine::default()),
        b"{}",
        current,
        source,
        &selected.id,
        "openai-response",
        "claude",
        Some(&selected),
        None,
    )
    .unwrap();
    assert_eq!(at(&output, "max_tokens"), json!(4096));
    assert_eq!(at(&output, "thinking.budget_tokens"), json!(4095));
    assert_eq!(at(&output, "thinking.type"), json!("enabled"));
    assert_eq!(at(&output, "thinking.display"), json!("summarized"));
}

struct LookupOwner(Arc<AtomicUsize>);
impl ModelInfoResolver for LookupOwner {
    fn lookup_model_info(&self, model: &str, provider: &str) -> Option<ModelInfo> {
        assert_eq!((model, provider), ("lookup-owned", "gemini"));
        self.0.fetch_add(1, Ordering::SeqCst);
        Some(ModelInfo {
            id: "lookup-owned",
            provider_type: "gemini",
            user_defined: false,
            max_completion_tokens: 0,
            thinking: Some(ThinkingSupport {
                min: None,
                max: None,
                zero_allowed: true,
                dynamic_allowed: true,
                levels: &["high"],
            }),
        })
    }
}

#[test]
fn candidate_thinking_pipeline_unselected_models_use_owner_registry_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let pipeline = pipeline(ThinkingEngine::new(Arc::new(LookupOwner(Arc::clone(
        &calls,
    )))));
    let output = run(
        &pipeline,
        b"{}",
        b"{}",
        b"{}",
        "lookup-owned(high)",
        "gemini",
        "gemini",
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        at(&output, "generationConfig.thinkingConfig.thinkingLevel"),
        json!("high")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let alias_output = run(
        &self::pipeline(ThinkingEngine::default()),
        br#"{"reasoning":{"effort":"low"}}"#,
        b"{}",
        b"{}",
        "unlisted-owned(high)",
        "openai-response",
        "openai-response",
        None,
        None,
    )
    .unwrap();
    assert_eq!(at(&alias_output, "reasoning.effort"), json!("high"));
}

struct ViewPlugin;
impl ProviderApplier for ViewPlugin {
    fn apply(
        &self,
        _: &[u8],
        _: &ThinkingConfig,
        _: Option<&ModelInfo>,
    ) -> Result<Vec<u8>, ThinkingError> {
        panic!("owned metadata must not use the static plugin entry point")
    }
    fn apply_model_info(
        &self,
        _: &[u8],
        config: &ThinkingConfig,
        info: Option<&ModelInfoView<'_>>,
    ) -> Result<Vec<u8>, ThinkingError> {
        let info = info.unwrap();
        Ok(
            serde_json::to_vec(&json!({"id":info.id, "level":config.level.as_str(),
            "compat":info.is_compat, "updates":info.support_configuration_update}))
            .unwrap(),
        )
    }
}
struct LegacyPlugin;
impl ProviderApplier for LegacyPlugin {
    fn apply(
        &self,
        body: &[u8],
        _: &ThinkingConfig,
        _: Option<&ModelInfo>,
    ) -> Result<Vec<u8>, ThinkingError> {
        Ok(body.to_vec())
    }
}

#[test]
fn candidate_thinking_pipeline_plugin_receives_owned_metadata_or_fails_explicitly() {
    let engine = ThinkingEngine::default();
    assert!(engine.register_plugin_provider("view-owner", "owned-plugin", 1, Arc::new(ViewPlugin)));
    assert!(engine.register_plugin_provider(
        "legacy-owner",
        "legacy-plugin",
        1,
        Arc::new(LegacyPlugin)
    ));
    let pipeline = pipeline(engine);
    let mut selected = owned_model("private-plugin-model", "owned-plugin", &["high"]);
    selected.is_compat = true;
    selected.support_configuration_update = true;
    let output = run(
        &pipeline,
        b"{}",
        b"{}",
        b"{}",
        "private-plugin-model(high)",
        "owned-plugin",
        "owned-plugin",
        Some(&selected),
        None,
    )
    .unwrap();
    assert_eq!(at(&output, "id"), json!("private-plugin-model"));
    assert_eq!(at(&output, "level"), json!("high"));
    assert_eq!(at(&output, "compat"), json!(true));
    assert_eq!(at(&output, "updates"), json!(true));
    selected.provider_type = "legacy-plugin".to_owned();
    let error = run(
        &pipeline,
        b"{}",
        b"{}",
        b"{}",
        "private-plugin-model(high)",
        "legacy-plugin",
        "legacy-plugin",
        Some(&selected),
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ProviderMismatch);
    assert_eq!(error.model, "private-plugin-model");
}

#[test]
fn candidate_thinking_pipeline_static_unsigned_and_owned_signed_bounds_are_lossless() {
    let static_info = ModelInfo {
        id: "claude-large-static",
        provider_type: "claude",
        user_defined: false,
        max_completion_tokens: 0,
        thinking: Some(ThinkingSupport {
            min: Some(u64::MAX),
            max: Some(u64::MAX),
            zero_allowed: false,
            dynamic_allowed: false,
            levels: &[],
        }),
    };
    let output = apply_summary_config_for_resolved_model(
        b"{}",
        "claude",
        static_info.id,
        Some(&static_info),
        &SummaryConfig {
            mode: SummaryMode::Enabled,
            detail: "auto".to_owned(),
        },
    );
    assert_eq!(at(&output, "thinking.budget_tokens"), json!(u64::MAX));
    let mut selected = owned_model("runtime-signed-bounds", "gemini", &[]);
    selected.thinking.as_mut().unwrap().min = i64::MIN;
    selected.thinking.as_mut().unwrap().max = i64::MAX;
    let view = ModelInfoView::from(&selected);
    let support = view.thinking.as_ref().unwrap();
    assert_eq!(support.min, Some(i128::from(i64::MIN)));
    assert_eq!(support.max, Some(i128::from(i64::MAX)));
    let source = br#"{"generationConfig":{"thinkingConfig":{"thinkingBudget":10}}}"#;
    let output = run(
        &pipeline(ThinkingEngine::default()),
        b"{}",
        source,
        source,
        &selected.id,
        "gemini",
        "gemini",
        Some(&selected),
        None,
    )
    .unwrap();
    assert_eq!(
        at(&output, "generationConfig.thinkingConfig.thinkingBudget"),
        json!(10)
    );
}

#[test]
fn candidate_thinking_pipeline_unselected_embedded_model_keeps_private_capabilities() {
    let engine = ThinkingEngine::default();
    engine.register_provider("codex", Arc::new(ViewPlugin));
    let output = run(
        &pipeline(engine),
        b"{}",
        b"{}",
        b"{}",
        "gpt-6-astra(high)",
        "codex",
        "codex",
        None,
        None,
    )
    .unwrap();
    assert_eq!(at(&output, "id"), json!("gpt-6-astra"));
    assert_eq!(at(&output, "level"), json!("high"));
    assert_eq!(at(&output, "updates"), json!(true));
}

#[test]
fn candidate_thinking_pipeline_live_registry_preserves_provider_and_owner_scope() {
    use crate::internal::registry::{
        ModelRegistry, RegistryModelInfo, RegistryThinkingSupport, StaticModelsCatalog,
    };
    let make_info = |provider: &str, level: &str, updates: bool| RegistryModelInfo {
        id: "live-scoped-model".to_owned(),
        provider_type: provider.to_owned(),
        support_configuration_update: updates,
        thinking: Some(RegistryThinkingSupport {
            levels: vec![level.to_owned()],
            ..Default::default()
        }),
        ..Default::default()
    };
    let left = Arc::new(ModelRegistry::new(Arc::new(StaticModelsCatalog::default())));
    let right = Arc::new(ModelRegistry::new(Arc::new(StaticModelsCatalog::default())));
    left.register_client("google", "gemini", &[make_info("gemini", "left", false)]);
    left.register_client("responses", "codex", &[make_info("codex", "high", true)]);
    right.register_client("google", "gemini", &[make_info("gemini", "right", false)]);
    let engine = ThinkingEngine::new(Arc::new(super::RegistryModelInfoResolver::new(Arc::clone(
        &left,
    ))));
    engine.register_provider("codex", Arc::new(ViewPlugin));
    let left_pipeline = pipeline(engine);
    let right_pipeline = pipeline(ThinkingEngine::new(Arc::new(
        super::RegistryModelInfoResolver::new(right),
    )));
    let current = br#"{"generationConfig":{"thinkingConfig":{"thinkingLevel":"left"}}}"#;
    let output = run(
        &left_pipeline,
        b"{}",
        current,
        current,
        "live-scoped-model",
        "gemini",
        "gemini",
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        at(&output, "generationConfig.thinkingConfig.thinkingLevel"),
        json!("left")
    );
    let error = run(
        &right_pipeline,
        b"{}",
        current,
        current,
        "live-scoped-model",
        "gemini",
        "gemini",
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::LevelNotSupported);
    assert!(error.message.contains("right"));
    let output = run(
        &left_pipeline,
        b"{}",
        b"{}",
        b"{}",
        "live-scoped-model(high)",
        "codex",
        "codex",
        None,
        None,
    )
    .unwrap();
    assert_eq!(at(&output, "updates"), json!(true));

    // Replacing the owner registration must affect the next request; the
    // thinking bridge must not cache the previous model capability.
    left.register_client("responses", "codex", &[make_info("codex", "high", false)]);
    let output = run(
        &left_pipeline,
        b"{}",
        b"{}",
        b"{}",
        "live-scoped-model(high)",
        "codex",
        "codex",
        None,
        None,
    )
    .unwrap();
    assert_eq!(at(&output, "updates"), json!(false));
}

struct LegacyBoundsOwner;
impl ModelInfoResolver for LegacyBoundsOwner {
    fn lookup_model_info(&self, model: &str, provider: &str) -> Option<ModelInfo> {
        assert_eq!((model, provider), ("legacy-owner-bounds", "claude"));
        Some(ModelInfo {
            id: "legacy-owner-bounds",
            provider_type: "claude",
            user_defined: false,
            max_completion_tokens: 0,
            thinking: Some(ThinkingSupport {
                min: Some(u64::MAX),
                max: Some(u64::MAX),
                zero_allowed: false,
                dynamic_allowed: false,
                levels: &[],
            }),
        })
    }
}
struct LegacyBoundsApplier;
impl ProviderApplier for LegacyBoundsApplier {
    fn apply(
        &self,
        _: &[u8],
        _: &ThinkingConfig,
        info: Option<&ModelInfo>,
    ) -> Result<Vec<u8>, ThinkingError> {
        let support = info.unwrap().thinking.as_ref().unwrap();
        assert_eq!(support.min, Some(u64::MAX));
        assert_eq!(support.max, Some(u64::MAX));
        Ok(br#"{"preserved":true}"#.to_vec())
    }
}

#[test]
fn candidate_thinking_pipeline_legacy_resolver_retains_exact_static_bounds() {
    let engine = ThinkingEngine::new(Arc::new(LegacyBoundsOwner));
    engine.register_provider("claude", Arc::new(LegacyBoundsApplier));
    let output = run(
        &pipeline(engine),
        b"{}",
        b"{}",
        b"{}",
        "legacy-owner-bounds(high)",
        "claude",
        "claude",
        None,
        None,
    )
    .unwrap();
    assert_eq!(at(&output, "preserved"), json!(true));
}
