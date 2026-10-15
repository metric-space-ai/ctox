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

// ref: internal/registry/models/models.json @ 16d98881d4bb37adaa827599e4be8f5154e81646

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
    // Network catalogs retain private capabilities omitted by public model JSON.
    let mut next_json: serde_json::Value =
        serde_json::from_str(include_str!("models/models.json")).unwrap();
    next_json["kimi"][0]["description"] = next.kimi[0].description.clone().into();
    let store = Arc::new(ModelCatalogStore::new(initial));
    let registry = ModelRegistry::from_store(Arc::clone(&store));
    let source = Arc::new(Source(Mutex::new(HashMap::from([
        ("transport-error".to_owned(), Err("offline".to_owned())),
        (
            "invalid".to_owned(),
            Ok(br#"{"claude": [{"id": ""}]}"#.to_vec()),
        ),
        (
            "valid".to_owned(),
            Ok(serde_json::to_vec(&next_json).unwrap()),
        ),
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
