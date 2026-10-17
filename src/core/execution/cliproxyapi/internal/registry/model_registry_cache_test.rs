// ref: internal/registry/model_registry_cache_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use serde_json::Value;

use super::*;

fn registry() -> ModelRegistry {
    ModelRegistry::new(Arc::new(embedded_models_catalog().unwrap()))
}

fn model(id: &str) -> RegistryModelInfo {
    RegistryModelInfo {
        id: id.to_owned(),
        ..RegistryModelInfo::default()
    }
}

#[test]
fn available_models_returns_cloned_cached_snapshots() {
    let registry = registry();
    registry.register_client(
        "client-1",
        "OpenAI",
        &[RegistryModelInfo {
            id: "m1".to_owned(),
            owned_by: "team-a".to_owned(),
            display_name: "Model One".to_owned(),
            ..RegistryModelInfo::default()
        }],
    );
    let mut first = registry.available_models("openai");
    first[0].insert("id".to_owned(), Value::String("mutated".to_owned()));
    first[0].insert(
        "display_name".to_owned(),
        Value::String("Mutated".to_owned()),
    );
    let second = registry.available_models("openai");
    assert_eq!(second[0]["id"], "m1");
    assert_eq!(second[0]["display_name"], "Model One");
}

#[test]
fn registry_changes_invalidate_cache() {
    let registry = registry();
    let mut info = model("m1");
    info.display_name = "Model One".to_owned();
    registry.register_client("client-1", "OpenAI", &[info.clone()]);
    assert_eq!(
        registry.available_models("openai")[0]["display_name"],
        "Model One"
    );
    info.display_name = "Model One Updated".to_owned();
    registry.register_client("client-1", "OpenAI", &[info]);
    assert_eq!(
        registry.available_models("openai")[0]["display_name"],
        "Model One Updated"
    );
    registry.suspend_client_model("client-1", "m1", "manual");
    assert!(registry.available_models("openai").is_empty());
    registry.resume_client_model("client-1", "m1");
    assert_eq!(registry.available_models("openai").len(), 1);
}
