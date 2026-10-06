// ref: internal/registry/model_definitions.go:88-211 @ d7914afd
// ref: internal/registry/devin_models.go:40-67 @ d7914afd
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{RegistryModelInfo, RegistryThinkingSupport};

pub(super) fn devin_builtin_swe16_slow() -> RegistryModelInfo {
    RegistryModelInfo {
        id: "devin/swe-1-6-slow".into(),
        object: "model".into(),
        provider_type: "devin".into(),
        owned_by: "cognition".into(),
        display_name: "SWE-1.6 Slow".into(),
        context_length: 200_000,
        max_completion_tokens: 64_000,
        input_token_limit: 200_000,
        output_token_limit: 64_000,
        supported_input_modalities: vec!["text".into(), "image".into()],
        supported_output_modalities: vec!["text".into()],
        supported_generation_methods: vec!["generateContent".into(), "countTokens".into()],
        ..RegistryModelInfo::default()
    }
}

pub(super) fn static_devin_models() -> Vec<RegistryModelInfo> {
    let definitions: &[(&str, &str, &str, usize, usize, &[&str])] = &[
        (
            "devin/swe-2",
            "cognition",
            "SWE-2",
            262_000,
            128_000,
            &["medium", "high", "max"],
        ),
        (
            "devin/claude-fable-5-1",
            "anthropic",
            "Claude Fable 5.1",
            1_000_000,
            64_000,
            &["low", "medium", "high", "xhigh", "max"],
        ),
        (
            "devin/gpt-6-astra",
            "openai",
            "GPT-6 Astra",
            1_000_000,
            64_000,
            &["low", "medium", "high", "xhigh", "max"],
        ),
        (
            "devin/glm-5-2",
            "zhipu",
            "GLM-5.2",
            200_000,
            64_000,
            &["none", "high"],
        ),
        (
            "devin/glm-5-3",
            "zhipu",
            "GLM-5.3",
            1_048_576,
            128_000,
            &["low", "high", "max"],
        ),
        (
            "devin/glm-5-3-flash",
            "zhipu",
            "GLM-5.3 Flash",
            1_000_000,
            128_000,
            &["low", "high", "max"],
        ),
        (
            "devin/gpt-5-6-sol",
            "openai",
            "GPT-5.6 Sol",
            1_000_000,
            128_000,
            &["none", "low", "medium", "high", "xhigh", "max"],
        ),
        (
            "devin/gemini-3-8-flash",
            "google",
            "Gemini 3.8 Flash",
            1_048_576,
            65_536,
            &["low", "medium", "high"],
        ),
        (
            "devin/grok-4-6",
            "xai",
            "Grok 4.6",
            500_000,
            131_072,
            &["low", "medium", "high", "xhigh"],
        ),
        (
            "devin/deepseek-v4-flash",
            "deepseek",
            "DeepSeek V4 Flash",
            1_048_576,
            64_000,
            &["high", "max"],
        ),
        (
            "devin/deepseek-v4-1-flash",
            "deepseek",
            "DeepSeek V4.1 Flash",
            1_048_576,
            64_000,
            &["high", "max"],
        ),
    ];
    std::iter::once(devin_builtin_swe16_slow())
        .chain(
            definitions
                .iter()
                .map(
                    |(id, owner, display, context, completion, levels)| RegistryModelInfo {
                        id: (*id).into(),
                        provider_type: "devin".into(),
                        owned_by: (*owner).into(),
                        display_name: (*display).into(),
                        context_length: *context,
                        max_completion_tokens: *completion,
                        thinking: Some(RegistryThinkingSupport {
                            levels: levels.iter().map(|level| (*level).into()).collect(),
                            ..RegistryThinkingSupport::default()
                        }),
                        ..RegistryModelInfo::default()
                    },
                ),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::model_definitions::with_devin_builtins;
    use super::super::{
        embedded_models_catalog, lookup_static_registry_model_info, models_for_channel,
    };
    use super::*;

    #[test]
    fn candidate_devin_builtin_metadata_and_static_lookup_preserve_provider_provenance() {
        let models = static_devin_models();
        assert_eq!(models.len(), 12);
        assert_eq!(models[0].id, "devin/swe-1-6-slow");
        let slow = &models[0];
        assert_eq!(slow.provider_type, "devin");
        assert_eq!(slow.owned_by, "cognition");
        assert_eq!(
            (slow.context_length, slow.input_token_limit),
            (200_000, 200_000)
        );
        assert_eq!(
            (slow.max_completion_tokens, slow.output_token_limit),
            (64_000, 64_000)
        );
        assert_eq!(slow.supported_input_modalities, ["text", "image"]);
        assert_eq!(slow.supported_output_modalities, ["text"]);
        assert_eq!(
            slow.supported_generation_methods,
            ["generateContent", "countTokens"]
        );

        let mut catalog = embedded_models_catalog().unwrap();
        catalog.devin.clear();
        let selected = lookup_static_registry_model_info(&catalog, "devin/gpt-6-astra").unwrap();
        assert_eq!(selected.provider_type, "devin");
        assert_eq!(selected.owned_by, "openai");
        assert_eq!(
            (selected.context_length, selected.max_completion_tokens),
            (1_000_000, 64_000)
        );
        assert_eq!(
            selected.thinking.unwrap().levels,
            ["low", "medium", "high", "xhigh", "max"]
        );
        let direct = lookup_static_registry_model_info(&catalog, "gpt-6-astra").unwrap();
        assert_eq!(direct.provider_type, "openai");
        assert_eq!(direct.context_length, 272_000);
        assert!(direct.support_configuration_update);
        assert!(lookup_static_registry_model_info(&catalog, "DEVIN/GPT-6-ASTRA").is_none());
    }

    #[test]
    fn candidate_devin_builtin_replacement_and_catalog_precedence() {
        let selected = with_devin_builtins(vec![
            RegistryModelInfo {
                id: " DEVIN/SWE-1-6-SLOW ".into(),
                context_length: 1,
                ..RegistryModelInfo::default()
            },
            RegistryModelInfo {
                id: "devin/custom".into(),
                context_length: 777,
                ..RegistryModelInfo::default()
            },
        ]);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].id, "devin/custom");
        assert_eq!(selected[1], devin_builtin_swe16_slow());
        let mut catalog = embedded_models_catalog().unwrap();
        catalog.devin = vec![RegistryModelInfo {
            id: "devin/gpt-6-astra".into(),
            provider_type: "devin".into(),
            context_length: 888,
            ..RegistryModelInfo::default()
        }];
        assert_eq!(
            lookup_static_registry_model_info(&catalog, "devin/gpt-6-astra")
                .unwrap()
                .context_length,
            888
        );
        let channel = models_for_channel(&catalog, "devin").unwrap();
        assert_eq!(channel.len(), 2);
        assert_eq!(channel[0].context_length, 888);
        assert_eq!(channel[1], devin_builtin_swe16_slow());
    }
}
