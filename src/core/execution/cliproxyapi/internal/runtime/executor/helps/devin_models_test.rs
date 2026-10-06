// ref: internal/runtime/executor/helps/devin_models_test.go:10-602
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Test source blob: d73914a640d73ff9f053d989b33b6fb561a525e7
// License: MIT (upstream); modifications AGPL-3.0-only

use super::devin_models::{
    has_devin_effort_suffix, normalize_devin_thinking_level, resolve_devin_chat_model_uid,
};
use crate::internal::registry::{
    embedded_models_catalog, DevinModelsStore, RegistryModelInfo, RegistryThinkingSupport,
    StaticModelsCatalog,
};

#[test]
fn candidate_devin_model_uid_effort_aliases_and_budget_boundaries() {
    for (level, budget, expected) in [
        (" minimal ", 64000, "minimal"),
        ("LOW", 64000, "low"),
        ("medium", 0, "medium"),
        ("high", 0, "high"),
        ("xhigh", 0, "xhigh"),
        ("max", 0, "max"),
        ("fast", 0, "fast"),
        ("none", 64000, "none"),
        ("off", 64000, "none"),
        ("disabled", 64000, "none"),
        ("auto", 1, "high"),
        ("Adaptive", 1, "high"),
        ("", -1, ""),
        ("", 0, ""),
        ("unknown", 0, ""),
        ("", 1, "low"),
        ("", 4096, "low"),
        ("", 4097, "medium"),
        ("", 16384, "medium"),
        ("", 16385, "high"),
        ("", 32768, "high"),
        ("", 32769, "max"),
        ("unknown", i64::MAX, "max"),
    ] {
        assert_eq!(
            normalize_devin_thinking_level(level, budget),
            expected,
            "{level:?}/{budget}"
        );
    }
}

#[test]
fn candidate_devin_model_uid_matches_upstream_examples() {
    let models = DevinModelsStore::from_embedded().unwrap();
    let catalog = embedded_models_catalog().unwrap();
    for (raw, effort, budget, expected) in [
        ("claude-fable-5-1-max", "", 0, "claude-fable-5-1-max"),
        ("swe-2-high", "", 0, "swe-2-high"),
        ("swe-2", "", 0, "swe-2-high"),
        ("swe-2", "minimal", 0, "swe-2-medium"),
        ("swe-2", "low", 0, "swe-2-medium"),
        ("swe-2", "xhigh", 0, "swe-2-max"),
        ("swe-2", "max", 0, "swe-2-max"),
        ("swe-2(max)", "medium", 0, "swe-2-max"),
        ("claude-fable-5-1", "", 64000, "claude-fable-5-1-max"),
        ("claude-fable-5-1", "", 2048, "claude-fable-5-1-low"),
        ("claude-fable-5-1", "xhigh", 0, "claude-fable-5-1-xhigh"),
        ("gpt-6-astra(high)", "", 0, "gpt-6-astra-high"),
        ("glm-5-3", "medium", 0, "glm-5-3-high"),
        ("devin/glm-5-3-flash", "", 0, "glm-5-3-flash-high"),
        ("devin/glm-5-3-flash:low", "", 0, "glm-5-3-flash-low"),
        ("devin/glm-5-3-flash(max)", "", 0, "glm-5-3-flash-max"),
        ("devin/gpt-5-6-sol:none", "", 0, "gpt-5-6-sol-none"),
        ("devin/gpt-5-6-sol", "none", 0, "gpt-5-6-sol-none"),
        ("devin/gpt-5-6-terra:none", "", 0, "gpt-5-6-terra-none"),
        (
            "devin/nemotron-3-ultra:none",
            "",
            0,
            "nemotron-3-ultra-none",
        ),
        ("devin/glm-5-2", "high", 0, "glm-5-2"),
        ("devin/glm-5-2:none", "", 0, "glm-5-2-none"),
        ("devin/glm-5-2(max)", "", 0, "glm-5-2-max"),
        ("devin/swe-2", "", 0, "swe-2-high"),
        ("Devin/swe-2(max)", "", 0, "swe-2-max"),
        ("devin/claude-fable-5-1", "", 0, "claude-fable-5-1-medium"),
        ("Devin/gpt-6-astra(high)", "", 0, "gpt-6-astra-high"),
        ("devin/swe-2-high", "", 0, "swe-2-high"),
        ("devin/gemini-3-8-flash", "", 0, "gemini-3-8-flash-high"),
        ("devin/gemini-3.8-flash(low)", "", 0, "gemini-3-8-flash-low"),
        ("devin/grok-4-6", "", 0, "grok-4-6-high"),
        ("devin/grok-4.6:xhigh", "", 0, "grok-4-6-xhigh"),
        ("devin/deepseek-v4-flash", "", 0, "deepseek-v4-flash-high"),
        (
            "devin/deepseek-v4.1-flash(max)",
            "",
            0,
            "deepseek-v4-1-flash-max",
        ),
        ("devin/swe-1-7", "", 0, "swe-1-7"),
        ("devin/swe-1-7:medium", "", 0, "swe-1-7-medium"),
        ("devin/claude-haiku-4-5", "", 0, "MODEL_PRIVATE_11"),
        ("devin/claude-sonnet-4-5", "", 0, "MODEL_PRIVATE_2"),
        ("devin/claude-sonnet-4-5:high", "", 0, "MODEL_PRIVATE_3"),
        ("devin/gpt-4-1", "", 0, "MODEL_CHAT_GPT_4_1_2025_04_14"),
        ("devin/gemini-3-flash", "", 0, "gemini-3-8-flash-high"),
        ("devin/gpt-5-6-luna", "", 0, "gpt-5-6-luna-low"),
        ("devin/gpt-5-6-luna(high)", "", 0, "gpt-5-6-luna-high"),
        ("devin/gpt-5-6-luna:none", "", 0, "gpt-5-6-luna-none"),
        ("devin/gpt-5-6-sol", "", 0, "gpt-5-6-sol-low"),
        ("devin/gpt-5-6-terra", "", 0, "gpt-5-6-terra-low"),
        ("devin/gpt-5-5", "", 0, "gpt-5-5-low"),
        ("devin/gpt-5-4", "", 0, "gpt-5-4-low"),
        ("devin/gpt-5-4-mini", "", 0, "gpt-5-4-mini-medium"),
        ("devin/gpt-5-3-codex", "", 0, "gpt-5-3-codex-medium"),
        ("devin/claude-opus-5", "", 0, "claude-opus-5-medium"),
        ("devin/claude-opus-4-8", "", 0, "claude-opus-4-8-medium"),
        ("devin/claude-opus-4-7", "", 0, "claude-opus-4-7-medium"),
        ("devin/claude-sonnet-5", "", 0, "claude-sonnet-5-medium"),
        ("devin/gemini-3-7-flash", "", 0, "gemini-3-7-flash-high"),
        ("devin/gemini-3-6-flash", "", 0, "gemini-3-6-flash-high"),
        ("devin/gemini-3-5-flash", "", 0, "gemini-3-5-flash-high"),
        ("devin/deepseek-v4-pro", "", 0, "deepseek-v4-pro-high"),
        ("devin/grok-4-5", "", 0, "grok-4-5-high"),
        ("devin/kimi-k3", "", 0, "kimi-k3-high"),
        ("devin/nemotron-3-ultra", "", 0, "nemotron-3-ultra-high"),
        ("devin/swe-1-6", "", 0, "swe-1-6"),
        ("devin/swe-1-6:fast", "", 0, "swe-1-6-fast"),
        ("devin/swe-1-6-slow", "", 0, "swe-1-6-slow"),
        ("swe-1-6-slow", "", 0, "swe-1-6-slow"),
        ("devin/swe-1-6-slow:low", "", 0, "swe-1-6-slow"),
        ("devin/swe-1-6-slow:high", "", 0, "swe-1-6-slow"),
        ("devin/kimi-k2-6", "", 0, "kimi-k2-6"),
        ("devin/kimi-k2-7", "", 0, "kimi-k2-7"),
        ("devin/claude-opus-4-6", "", 0, "claude-opus-4-6"),
        ("devin/claude-sonnet-4-6", "", 0, "claude-sonnet-4-6"),
        (
            "devin/claude-opus-4-6",
            "high",
            0,
            "claude-opus-4-6-thinking",
        ),
        (
            "devin/claude-sonnet-4-6",
            "high",
            0,
            "claude-sonnet-4-6-thinking",
        ),
        ("devin/claude-opus-4-6-1m", "", 0, "claude-opus-4-6-1m"),
        (
            "devin/claude-opus-4-6-1m",
            "high",
            0,
            "claude-opus-4-6-thinking-1m",
        ),
        ("devin/claude-sonnet-4-6-1m", "", 0, "claude-sonnet-4-6-1m"),
        (
            "devin/claude-sonnet-4-6-1m",
            "high",
            0,
            "claude-sonnet-4-6-thinking-1m",
        ),
        ("devin/glm-5-2-1m", "", 0, "glm-5-2-1m"),
        ("devin/glm-5-2-1m", "none", 0, "glm-5-2-none-1m"),
        ("devin/glm-5-2-1m", "max", 0, "glm-5-2-max-1m"),
        ("devin/MODEL_GPT_5_2", "", 0, "MODEL_GPT_5_2_LOW"),
        ("devin/MODEL_GPT_5_2", "none", 0, "MODEL_GPT_5_2_NONE"),
        ("devin/MODEL_GPT_5_2", "medium", 0, "MODEL_GPT_5_2_MEDIUM"),
        ("devin/MODEL_GPT_5_2", "high", 0, "MODEL_GPT_5_2_HIGH"),
        ("devin/MODEL_GPT_5_2", "xhigh", 0, "MODEL_GPT_5_2_XHIGH"),
        (
            "devin/MODEL_GOOGLE_GEMINI_3_0_FLASH",
            "",
            0,
            "MODEL_GOOGLE_GEMINI_3_0_FLASH_HIGH",
        ),
        (
            "devin/MODEL_GOOGLE_GEMINI_3_0_FLASH",
            "minimal",
            0,
            "MODEL_GOOGLE_GEMINI_3_0_FLASH_MINIMAL",
        ),
        (
            "devin/MODEL_GOOGLE_GEMINI_3_0_FLASH",
            "low",
            0,
            "MODEL_GOOGLE_GEMINI_3_0_FLASH_LOW",
        ),
        (
            "devin/MODEL_GOOGLE_GEMINI_3_0_FLASH",
            "medium",
            0,
            "MODEL_GOOGLE_GEMINI_3_0_FLASH_MEDIUM",
        ),
        (
            "devin/MODEL_GOOGLE_GEMINI_3_0_FLASH",
            "high",
            0,
            "MODEL_GOOGLE_GEMINI_3_0_FLASH_HIGH",
        ),
        (
            "devin/MODEL_CLAUDE_4_5_OPUS",
            "",
            0,
            "MODEL_CLAUDE_4_5_OPUS",
        ),
        (
            "devin/MODEL_CLAUDE_4_5_OPUS",
            "high",
            0,
            "MODEL_CLAUDE_4_5_OPUS_THINKING",
        ),
    ] {
        assert_eq!(
            resolve_devin_chat_model_uid(raw, effort, budget, &models, &catalog),
            expected,
            "{raw:?}/{effort:?}/{budget}"
        );
    }
}

#[test]
fn candidate_devin_model_uid_direct_variants_preserve_casing_and_override_body() {
    let models = DevinModelsStore::default();
    let catalog = StaticModelsCatalog::default();
    for direct in [
        "SWE-2-HIGH",
        "gpt-5.6-none",
        "model_X_LOW",
        "model_X_THINKING",
        "unknown-none-fast",
        "unknown-max-priority",
        "unknown-thinking-1m",
        "unknown-none-1m",
        "swe-1-6-slow",
    ] {
        let raw = format!("  DeViN/{direct}  ");
        assert!(has_devin_effort_suffix(&raw));
        assert_eq!(
            resolve_devin_chat_model_uid(&raw, "max", i64::MAX, &models, &catalog),
            direct
        );
    }
    for raw in ["", " \t\r\n"] {
        assert_eq!(
            resolve_devin_chat_model_uid(raw, "none", 0, &models, &catalog),
            "swe-2-high"
        );
    }
    assert_eq!(
        resolve_devin_chat_model_uid("Devin/ Unknown.Model:high", "max", 0, &models, &catalog),
        "unknown-model"
    );
    assert_eq!(
        resolve_devin_chat_model_uid("gpt-4.1(high)", "none", 0, &models, &catalog),
        "MODEL_CHAT_GPT_4_1_2025_04_14"
    );
}

#[test]
fn candidate_devin_model_uid_all_catalog_models_have_compatible_variants() {
    let models = DevinModelsStore::from_embedded().unwrap();
    let catalog = embedded_models_catalog().unwrap();
    let listed = models.models(&catalog);
    assert!(!listed.is_empty());
    for model in listed {
        let base = model.id.strip_prefix("devin/").unwrap_or(&model.id);
        for effort in ["", "none", "low", "medium", "high", "xhigh", "max"] {
            let resolved = resolve_devin_chat_model_uid(
                &format!("devin/{base}"),
                effort,
                0,
                &models,
                &catalog,
            );
            assert!(!resolved.is_empty(), "{base}/{effort}");
            if model
                .thinking
                .as_ref()
                .is_some_and(|thinking| !thinking.levels.is_empty())
                && ![
                    "swe-1-7",
                    "glm-5-2",
                    "glm-5-2-1m",
                    "swe-1-6-slow",
                    "model_claude_4_5_opus",
                ]
                .contains(&base)
            {
                assert!(
                    has_devin_effort_suffix(&resolved),
                    "{base}/{effort}: {resolved}"
                );
            }
        }
    }
}

#[test]
fn candidate_devin_model_uid_dynamic_updates_fallback_ties_and_scope() {
    let models = DevinModelsStore::default();
    let other_models = DevinModelsStore::default();
    let mut catalog = StaticModelsCatalog::default();
    catalog.devin = vec![RegistryModelInfo {
        id: "devin/custom.model".into(),
        thinking: Some(RegistryThinkingSupport {
            levels: vec!["low".into(), "high".into()],
            ..Default::default()
        }),
        ..Default::default()
    }];
    assert_eq!(
        resolve_devin_chat_model_uid("custom.model", "medium", 0, &models, &catalog),
        "custom-model-high"
    );
    models
        .load(
            br#"{"models":[{"id":"custom.model","thinking":{"levels":["minimal","max"]}}]}"#,
            "test",
        )
        .unwrap();
    assert_eq!(
        resolve_devin_chat_model_uid("custom.model", "medium", 0, &models, &catalog),
        "custom-model-minimal"
    );
    assert_eq!(
        resolve_devin_chat_model_uid("custom.model", "xhigh", 0, &models, &catalog),
        "custom-model-max"
    );
    assert_eq!(
        resolve_devin_chat_model_uid("custom.model", "none", 0, &models, &catalog),
        "custom-model-minimal"
    );
    assert_eq!(
        resolve_devin_chat_model_uid("custom.model", "fast", 0, &models, &catalog),
        "custom-model-minimal"
    );
    assert_eq!(
        resolve_devin_chat_model_uid("custom.model", "medium", 0, &other_models, &catalog),
        "custom-model-high"
    );
    assert!(models.load(b"{}", "invalid").is_err());
    assert_eq!(
        resolve_devin_chat_model_uid("custom.model", "medium", 0, &models, &catalog),
        "custom-model-minimal"
    );
    let third_models = DevinModelsStore::default();
    catalog.devin[0].thinking.as_mut().unwrap().levels = vec![" LOW ".into(), "HIGH".into()];
    assert_eq!(
        resolve_devin_chat_model_uid("custom.model", "medium", 0, &third_models, &catalog),
        "custom-model-HIGH"
    );
}
