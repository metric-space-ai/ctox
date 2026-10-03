// ref: internal/registry/model_definitions_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use sha2::{Digest, Sha256};

use super::*;

fn registry() -> ModelRegistry {
    ModelRegistry::new(Arc::new(embedded_models_catalog().unwrap()))
}

#[test]
fn model_override_headers_from_embedded_models() {
    const WANT_UA: &str =
        "codex-tui/0.144.0 (Mac OS 26.5.1; arm64) iTerm.app/3.6.11 (codex-tui; 0.144.0)";
    let registry = registry();
    assert_eq!(
        registry.model_override_headers("gpt-5.6-luna", "").unwrap()["user-agent"],
        WANT_UA
    );
    assert!(registry.model_override_headers("gpt-5.4", "").is_none());
}

#[test]
fn gemini_vertex_models_use_flash_lite_release_id() {
    let catalog = embedded_models_catalog().unwrap();
    let models = models_for_channel(&catalog, "vertex").unwrap();
    assert!(models
        .iter()
        .any(|model| model.id == "gemini-3.1-flash-lite"));
    assert!(!models
        .iter()
        .any(|model| model.id == "gemini-3.1-flash-lite-preview"));
}

#[test]
fn xai_builtins_include_video_preview_and_replace_case_insensitively() {
    let models = with_xai_builtins(vec![RegistryModelInfo {
        id: " GROK-IMAGINE-VIDEO-1.5-PREVIEW ".to_owned(),
        description: "stale".to_owned(),
        ..RegistryModelInfo::default()
    }]);
    assert_eq!(
        models
            .iter()
            .filter(|model| model.id == "grok-imagine-video-1.5-preview")
            .count(),
        1
    );
    assert_eq!(models.len(), 4);
}

#[test]
fn antigravity_web_search_requires_requested_provider_capability() {
    let registry = registry();
    registry.register_client(
        "test-antigravity",
        "antigravity",
        &[
            RegistryModelInfo {
                id: "gemini-route-test".to_owned(),
                ..RegistryModelInfo::default()
            },
            RegistryModelInfo {
                id: "gemini-web-search-test".to_owned(),
                supports_web_search: true,
                ..RegistryModelInfo::default()
            },
        ],
    );
    registry.register_client(
        "test-gemini",
        "gemini",
        &[RegistryModelInfo {
            id: "gemini-cross-provider-search".to_owned(),
            supports_web_search: true,
            ..RegistryModelInfo::default()
        }],
    );
    assert_eq!(
        antigravity_web_search_model_for(&registry, "gemini-route-test"),
        None
    );
    assert_eq!(
        antigravity_web_search_model_for(&registry, "gemini-route-test(high)"),
        None
    );
    assert_eq!(
        antigravity_web_search_model_for(&registry, "gemini-web-search-test"),
        Some("gemini-web-search-test".to_owned())
    );
    assert_eq!(
        antigravity_web_search_model_for(&registry, "gemini-cross-provider-search"),
        None
    );
}

// ref: internal/registry/models/models.json:3980-4030 @ d7914afd
#[test]
fn candidate_v13_antigravity_claude_catalog_keeps_provider_and_thinking_limits() {
    let catalog = embedded_models_catalog().unwrap();
    let models = models_for_channel(&catalog, "antigravity").unwrap();
    for (id, display) in [
        ("claude-opus-5-5-high", "Claude Opus 5.5 (High)"),
        ("claude-sonnet-5-5-high", "Claude Sonnet 5.5 (High)"),
    ] {
        let model = models.iter().find(|model| model.id == id).unwrap();
        assert_eq!(model.provider_type, "antigravity");
        assert_eq!(model.owned_by, "antigravity");
        assert_eq!(model.display_name, display);
        assert_eq!(model.name, id);
        assert_eq!(model.description, display);
        assert_eq!(model.context_length, 1_000_000);
        assert_eq!(model.max_completion_tokens, 128_000);
        assert_eq!(
            model.supported_input_modalities,
            vec!["text".to_owned(), "image".to_owned()]
        );
        assert_eq!(model.supported_output_modalities, vec!["text".to_owned()]);
        let thinking = model.thinking.as_ref().unwrap();
        assert_eq!((thinking.min, thinking.max), (1024, 64_000));
        assert!(thinking.zero_allowed && thinking.dynamic_allowed);
    }
    assert!(!models.iter().any(
        |model| ["claude-opus-4-6-thinking", "claude-sonnet-4-6"].contains(&model.id.as_str())
    ));
    assert!(models_for_channel(&catalog, "claude")
        .unwrap()
        .iter()
        .any(|model| model.id == "claude-sonnet-4-6"));
}

#[test]
fn complete_embedded_catalog_hash_channels_and_lookup_are_stable() {
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(include_str!("models/models.json").trim_end().as_bytes())
        ),
        "a872f4f35fe141383110a8477e2a9cc6ffc47f0b37fda8100133820a3787e32e"
    );
    let expected = [
        ("claude", 15),
        ("gemini", 12),
        ("vertex", 19),
        ("aistudio", 14),
        ("codex", 10),
        ("kimi", 8),
        ("antigravity", 12),
        ("xai", 13),
    ];
    for (channel, count) in expected {
        assert_eq!(
            models_for_channel(&embedded_models_catalog().unwrap(), channel)
                .unwrap()
                .len(),
            count,
            "{channel}"
        );
    }
    let sonnet = lookup_model_info("claude-sonnet-5", "claude").unwrap();
    assert_eq!(sonnet.max_completion_tokens, 128_000);
    assert!(sonnet.thinking.unwrap().dynamic_allowed);
}

#[test]
fn catalog_validation_rejects_empty_and_duplicate_ids() {
    let mut catalog = embedded_models_catalog().unwrap();
    catalog.claude[0].id = " ".to_owned();
    assert!(validate_models_catalog(&catalog).is_err());
    let mut catalog = embedded_models_catalog().unwrap();
    catalog.claude[1].id = catalog.claude[0].id.clone();
    assert!(validate_models_catalog(&catalog).is_err());
}

#[test]
fn model_refresh_detection_groups_codex_tiers_and_sink_replays_pending() {
    let old = embedded_models_catalog().unwrap();
    let mut new = old.clone();
    new.codex_team[0].description.push_str(" changed");
    new.codex_pro[0].description.push_str(" changed too");
    new.xai[0].description.push_str(" changed");
    assert_eq!(detect_changed_providers(&old, &new), ["codex", "xai"]);

    let sink = ModelRefreshSink::default();
    sink.notify(&[" Codex ".to_owned(), "codex".to_owned(), "XAI".to_owned()]);
    let captured = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let target = Arc::clone(&captured);
    sink.set_callback(Some(Arc::new(move |providers| {
        target.lock().unwrap().push(providers);
    })));
    assert_eq!(captured.lock().unwrap()[0], ["codex", "xai"]);
}

struct Source(Mutex<HashMap<String, Result<Vec<u8>, String>>>);

impl ModelsSource for Source {
    fn fetch<'a>(&'a self, source: &'a str, max_bytes: usize) -> ModelsFetchFuture<'a> {
        Box::pin(async move {
            assert_eq!(max_bytes, MAX_MODELS_CATALOG_SIZE);
            self.0
                .lock()
                .unwrap()
                .get(source)
                .cloned()
                .unwrap_or_else(|| Err("missing".to_owned()))
        })
    }
}

#[tokio::test]
async fn updater_falls_back_validates_commits_atomically_and_notifies() {
    let initial = embedded_models_catalog().unwrap();
    let mut next = initial.clone();
    next.kimi[0].description.push_str(" changed");
    let store = Arc::new(ModelCatalogStore::new(initial));
    let registry = ModelRegistry::from_store(Arc::clone(&store));
    let source = Arc::new(Source(Mutex::new(HashMap::from([
        ("transport-error".to_owned(), Err("offline".to_owned())),
        (
            "invalid".to_owned(),
            Ok(br#"{"claude": [{"id": ""}]}"#.to_vec()),
        ),
        ("valid".to_owned(), Ok(serde_json::to_vec(&next).unwrap())),
    ]))));
    let notifications = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let target = Arc::clone(&notifications);
    let sink = Arc::new(ModelRefreshSink::default());
    sink.set_callback(Some(Arc::new(move |providers| {
        target.lock().unwrap().push(providers);
    })));
    let updater = ModelsUpdater::new(
        Arc::clone(&store),
        source,
        vec![
            "transport-error".to_owned(),
            "invalid".to_owned(),
            "valid".to_owned(),
        ],
        sink,
    );
    let refreshed = updater.refresh_once().await.unwrap();
    assert_eq!(refreshed.source, "valid");
    assert_eq!(refreshed.changed_providers, ["kimi"]);
    assert_eq!(refreshed.revision, 2);
    assert_eq!(
        store.snapshot().catalog.kimi[0].description,
        next.kimi[0].description
    );
    assert_eq!(
        registry
            .lookup_model_info(&next.kimi[0].id, "")
            .unwrap()
            .description,
        next.kimi[0].description
    );
    assert_eq!(notifications.lock().unwrap()[0], ["kimi"]);
}
