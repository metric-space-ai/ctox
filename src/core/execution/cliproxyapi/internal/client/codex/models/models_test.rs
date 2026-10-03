// ref: internal/client/codex/models/models_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use super::*;

fn catalog(revision: u64) -> CodexModelCatalog {
    CodexModelCatalog::parse(
        br#"{"models":[
          {"slug":"gpt-5.5","display_name":"GPT 5.5","description":"default","priority":10,"supports_search_tool":true,"supported_reasoning_levels":[{"effort":"low"},{"effort":"ultra"}],"default_reasoning_level":"ultra","input_modalities":["text","image"],"apply_patch_tool_type":"freeform"},
          {"slug":"gpt-official","display_name":"Official","priority":20,"supports_search_tool":true}
        ]}"#,
        revision,
    )
    .unwrap()
}

fn model(value: Value) -> ModelMap {
    value.as_object().unwrap().clone()
}

fn empty_metadata(_: &str) -> Option<ModelMetadata> {
    None
}

#[test]
fn input_modalities_come_from_injected_registry_metadata() {
    let available = vec![
        model(json!({"id":"vision"})),
        model(json!({"id":"text"})),
        model(json!({"id":"image-endpoint"})),
    ];
    let source = |id: &str| {
        Some(match id {
            "vision" => ModelMetadata {
                supported_input_modalities: vec![
                    "text".into(),
                    "image".into(),
                    "audio".into(),
                    "IMAGE".into(),
                ],
                ..Default::default()
            },
            "text" => ModelMetadata {
                supported_input_modalities: vec!["text".into()],
                ..Default::default()
            },
            _ => ModelMetadata {
                model_type: "openai-image".into(),
                ..Default::default()
            },
        })
    };
    let models = catalog(1).build_models(&available, &source, None, false);
    let by_slug: std::collections::BTreeMap<_, _> = models
        .iter()
        .map(|entry| (entry["slug"].as_str().unwrap(), entry))
        .collect();
    assert_eq!(
        by_slug["vision"]["input_modalities"],
        json!(["text", "image"])
    );
    assert_eq!(by_slug["vision"]["supports_image_detail_original"], true);
    assert_eq!(by_slug["text"]["input_modalities"], json!(["text"]));
    assert!(by_slug["text"]
        .get("supports_image_detail_original")
        .is_none());
    assert_eq!(by_slug["image-endpoint"]["visibility"], "hide");
    assert!(by_slug["image-endpoint"].get("input_modalities").is_none());
}

#[test]
fn configured_display_name_applies_to_template() {
    let models = catalog(1).build_models(
        &[model(json!({"id":"gpt-5.5","display_name":"Configured"}))],
        &empty_metadata,
        None,
        false,
    );
    assert_eq!(models[0]["display_name"], "Configured");
}

#[test]
fn search_tool_requires_template_and_codex_only_providers() {
    let available = vec![
        model(json!({"id":"custom"})),
        model(json!({"id":"gpt-5.5"})),
        model(json!({"id":"gpt-official"})),
    ];
    let providers = |id: &str| match id {
        "gpt-5.5" => vec!["codex".to_owned()],
        "gpt-official" => vec!["codex".to_owned(), "openai".to_owned()],
        _ => vec!["codex".to_owned()],
    };
    let models = catalog(1).build_models(&available, &empty_metadata, Some(&providers), false);
    let by_slug: std::collections::BTreeMap<_, _> = models
        .iter()
        .map(|entry| (entry["slug"].as_str().unwrap(), entry))
        .collect();
    assert_eq!(by_slug["custom"]["supports_search_tool"], false);
    assert_eq!(by_slug["gpt-5.5"]["supports_search_tool"], true);
    assert_eq!(by_slug["gpt-official"]["supports_search_tool"], false);
}

#[test]
fn ultra_reasoning_effort_is_preserved() {
    let models = catalog(1).build_models(
        &[model(json!({"id":"gpt-5.5"}))],
        &empty_metadata,
        None,
        false,
    );
    assert_eq!(models[0]["default_reasoning_level"], "ultra");
    assert_eq!(
        models[0]["supported_reasoning_levels"][1]["effort"],
        "ultra"
    );
}

#[test]
fn catalog_revision_is_value_scoped_not_global_cache() {
    assert_eq!(catalog(7).revision(), 7);
    assert_eq!(catalog(8).revision(), 8);
}

#[test]
fn multi_agent_version_is_only_added_when_enabled() {
    let disabled = catalog(1).build_models(
        &[model(json!({"id":"custom"}))],
        &empty_metadata,
        None,
        false,
    );
    assert!(disabled[0].get("multi_agent_version").is_none());
    let enabled = catalog(1).build_models(
        &[model(json!({"id":"custom"}))],
        &empty_metadata,
        None,
        true,
    );
    assert_eq!(enabled[0]["multi_agent_version"], "v2");
}

#[test]
fn max_context_length_override_wins() {
    let available = vec![model(
        json!({"id":"custom","context_length":100,"max_context_length":4096}),
    )];
    let metadata = |_: &str| {
        Some(ModelMetadata {
            context_length: 2048,
            ..Default::default()
        })
    };
    let models = catalog(1).build_models(&available, &metadata, None, false);
    assert_eq!(models[0]["context_window"], 4096);
    assert_eq!(models[0]["max_context_window"], 4096);
}

#[test]
fn non_template_priorities_are_stable_by_display_name() {
    let available = vec![
        model(json!({"id":"z","display_name":"Alpha"})),
        model(json!({"id":"a","display_name":"Beta"})),
    ];
    let models = catalog(1).build_models(&available, &empty_metadata, None, false);
    assert_eq!(models[0]["slug"], "z");
    assert_eq!(models[1]["slug"], "a");
    assert!(models[0]["priority"].as_i64() < models[1]["priority"].as_i64());
}

#[test]
fn response_has_expected_envelope() {
    let response = catalog(1).build_response(
        &[model(json!({"id":"gpt-5.5"}))],
        &empty_metadata,
        None,
        false,
    );
    assert_eq!(response["models"].as_array().unwrap().len(), 1);
}

#[test]
fn candidate_apply_patch_template_fallback_and_exact_capability_override() {
    let available = vec![model(json!({"id":"custom"}))];
    let inherited = catalog(1).build_models(&available, &empty_metadata, None, false);
    for key in ["apply_patch_tool_type", "upgrade", "availability_nux"] {
        assert_eq!(inherited[0][key], Value::Null);
    }
    let official = vec![model(json!({"id":"gpt-5.5"}))];
    let inherited = catalog(1).build_models(&official, &empty_metadata, None, false);
    assert_eq!(inherited[0]["apply_patch_tool_type"], "freeform");
    for codex_only in [false, true] {
        let providers = |_: &str| {
            if codex_only {
                vec!["codex".into()]
            } else {
                vec!["codex".into(), "openai".into()]
            }
        };
        let entries = catalog(1).build_models(&official, &empty_metadata, Some(&providers), false);
        assert_eq!(
            entries[0]["apply_patch_tool_type"],
            if codex_only {
                json!("freeform")
            } else {
                Value::Null
            }
        );
        if !codex_only {
            assert_eq!(entries[0]["upgrade"], Value::Null);
            assert_eq!(entries[0]["availability_nux"], Value::Null);
        }
    }
    for supported in [false, true] {
        let resolver = |id: &str| {
            assert_eq!(id, "custom");
            supported
        };
        let models = catalog(1).build_models_with_apply_patch_capability(
            &available,
            &empty_metadata,
            None,
            false,
            Some(&resolver),
        );
        assert_eq!(
            models[0]["apply_patch_tool_type"],
            if supported {
                json!("freeform")
            } else {
                Value::Null
            }
        );
    }
}

#[test]
fn candidate_apply_patch_non_text_models_cannot_inherit_conversation_tools() {
    for id in [
        "gpt-image-2.5-flare",
        "gpt-image-2.5-sunburst",
        "gpt-image-2.5",
        "grok-imagine-image-2.0",
        "grok-imagine-video-1.5",
        "custom/gpt-image-2.5",
        "custom/grok-imagine-video-1.5",
    ] {
        let available = vec![model(json!({"id":id}))];
        let forbidden =
            |_: &str| -> bool { panic!("non-text model must not query routing capability") };
        let models = catalog(1).build_models_with_apply_patch_capability(
            &available,
            &empty_metadata,
            None,
            false,
            Some(&forbidden),
        );
        assert_eq!(models[0]["visibility"], "hide", "{id}");
        assert_eq!(models[0]["apply_patch_tool_type"], Value::Null, "{id}");
    }
    for template in [
        json!({"apply_patch_tool_type":"freeform", "input_modalities":["image"]}),
        json!({"apply_patch_tool_type":"freeform", "visibility":"hide"}),
    ] {
        let mut template = model(template);
        template.insert("slug".into(), json!("non-text"));
        let raw = serde_json::to_vec(&json!({"models":[{"slug":"gpt-5.5"}, template]})).unwrap();
        let catalog = CodexModelCatalog::parse(&raw, 1).unwrap();
        let entries = catalog.build_models(
            &[model(json!({"id":"non-text"}))],
            &empty_metadata,
            None,
            false,
        );
        assert_eq!(entries[0]["apply_patch_tool_type"], Value::Null);
    }
    let catalog = CodexModelCatalog::parse(
        br#"{"models":[{"slug":"gpt-5.5","apply_patch_tool_type":"freeform","visibility":"hide","input_modalities":["text"]}]}"#, 1,
    ).unwrap();
    let entries = catalog.build_models(
        &[model(json!({"id":"gpt-5.5"}))],
        &empty_metadata,
        None,
        false,
    );
    assert_eq!(entries[0]["apply_patch_tool_type"], "freeform");
}
