// ref: internal/registry/model_registry.go:29-33,107-121 @ d7914afd
// ref: internal/registry/model_definitions.go @ d7914afd
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;

#[test]
fn candidate_v8_catalog_decodes_private_capabilities_without_public_leaks() {
    let info: RegistryModelInfo = serde_json::from_str(
        r#"{"id":"private","type":"codex","support_configuration_update":true,"native_capabilities":{"web_search":false}}"#,
    ).unwrap();
    assert!(info.support_configuration_update);
    assert_eq!(
        info.native_capabilities.as_ref().unwrap().web_search,
        Some(false)
    );
    let public = serde_json::to_value(&info).unwrap();
    assert!(public.get("support_configuration_update").is_none());
    assert!(public.get("native_capabilities").is_none());
    assert_eq!(public["id"], "private");
    assert!(serde_json::from_str::<RegistryModelInfo>(
        r#"{"id":"bad","support_configuration_update":"true"}"#,
    )
    .is_err());
    let mut independent = info.clone();
    independent.native_capabilities.as_mut().unwrap().web_search = None;
    independent.support_configuration_update = false;
    assert_eq!(
        info.native_capabilities.as_ref().unwrap().web_search,
        Some(false)
    );
    assert!(info.support_configuration_update);

    let public_models = static_model_definitions_by_channel("codex")
        .unwrap()
        .unwrap();
    for model in public_models.as_array().unwrap() {
        assert!(model.get("native_capabilities").is_none());
        assert!(model.get("support_configuration_update").is_none());
    }
}

#[test]
fn candidate_v8_catalog_private_changes_refresh_the_exact_provider() {
    let old = embedded_models_catalog().unwrap();
    let mut changed = old.clone();
    changed.codex_plus[0].support_configuration_update =
        !changed.codex_plus[0].support_configuration_update;
    assert_eq!(detect_changed_providers(&old, &changed), ["codex"]);
    let mut changed = old.clone();
    changed.codex_team[0]
        .native_capabilities
        .as_mut()
        .unwrap()
        .web_search = Some(false);
    assert_eq!(detect_changed_providers(&old, &changed), ["codex"]);
    let mut changed = old.clone();
    changed.meta[0].native_capabilities = Some(NativeCapabilities {
        web_search: Some(false),
    });
    assert_eq!(detect_changed_providers(&old, &changed), ["meta"]);
    let mut changed = old.clone();
    changed.devin.push(RegistryModelInfo {
        id: "devin/test".into(),
        ..RegistryModelInfo::default()
    });
    assert_eq!(detect_changed_providers(&old, &changed), ["devin"]);
}

#[test]
fn candidate_v8_catalog_owned_lookup_keeps_new_models_and_native_metadata() {
    let selected =
        crate::internal::modelconfig::resolve_model_info(" gpt-6-astra(high) ", "codex", None);
    assert_eq!(selected.id, "gpt-6-astra(high)");
    assert_eq!(selected.provider_type, "codex");
    assert!(selected.support_configuration_update);
    assert_eq!(selected.native_capabilities.unwrap().web_search, Some(true));
    assert_eq!(selected.context_length, 272_000);
    assert!(!selected.user_defined && !selected.is_compat);

    // This new Claude entry was absent from the old lightweight static ID list.
    let selected =
        crate::internal::modelconfig::resolve_model_info("claude-fable-5-1(high)", "claude", None);
    assert_eq!(selected.context_length, 1_000_000);
    assert_eq!(selected.max_completion_tokens, 128_000);
    assert_eq!(
        selected.thinking.unwrap().levels,
        ["low", "medium", "high", "xhigh", "max"]
    );

    let catalog = embedded_models_catalog().unwrap();
    assert_eq!(
        models_for_channel(&catalog, "meta"),
        models_for_channel(&catalog, "muse")
    );
    assert_eq!(
        models_for_channel(&catalog, "gemini"),
        models_for_channel(&catalog, "gemini-interactions")
    );
    assert!(models_for_channel(&catalog, "meta")
        .unwrap()
        .iter()
        .any(|m| m.id == "muse-spark-1.3"));
}

#[test]
fn candidate_v8_catalog_builtin_metadata_and_order_match_upstream() {
    let codex = with_codex_builtins(vec![]);
    assert_eq!(
        codex.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        [
            "gpt-image-1.5",
            "gpt-image-2",
            "gpt-image-2.5-flare",
            "gpt-image-2.5-sunburst",
            "gpt-image-2.5",
        ]
    );
    for model in &codex {
        assert_eq!(model.created, 1_704_067_200);
        assert_eq!(model.object, "model");
        assert_eq!(model.owned_by, "openai");
        assert_eq!(model.provider_type, "openai");
        assert_eq!(model.version, model.id);
        assert!(model.thinking.is_none());
    }
    assert_eq!(codex[2].display_name, "GPT Image 2.5 Flare");
    assert_eq!(codex[3].display_name, "GPT Image 2.5 Sunburst");

    let xai = with_xai_builtins(vec![]);
    assert_eq!(
        xai.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        [
            "grok-imagine-image",
            "grok-imagine-image-quality",
            "grok-imagine-image-2.0",
            "grok-imagine-video",
            "grok-imagine-video-1.5",
            "grok-imagine-video-1.5-preview",
        ]
    );
    for model in &xai {
        assert_eq!(
            model.created,
            if model.id == "grok-imagine-image-2.0" {
                1_786_060_800
            } else {
                1_735_689_600
            }
        );
        assert_eq!(model.object, "model");
        assert_eq!(model.owned_by, "xai");
        assert_eq!(model.provider_type, "xai");
        assert_eq!(model.name, model.id);
    }
    assert_eq!(xai[4].display_name, "Grok Imagine Video 1.5");
    assert_eq!(
        xai[5].description,
        "Compatibility alias for the xAI Grok video generation model."
    );
}
