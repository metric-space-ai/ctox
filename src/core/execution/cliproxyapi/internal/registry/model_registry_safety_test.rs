// ref: internal/registry/model_registry_safety_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

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
fn model_info_models_for_client_and_provider_results_are_deep_clones() {
    let registry = registry();
    let mut info = model("m1");
    info.display_name = "Model One".to_owned();
    info.thinking = Some(RegistryThinkingSupport {
        min: 1,
        max: 2,
        levels: vec!["low".to_owned(), "high".to_owned()],
        ..RegistryThinkingSupport::default()
    });
    registry.register_client("client-1", "gemini", &[info]);

    let mut first = registry.model_info("m1", "gemini").unwrap();
    first.display_name = "mutated".to_owned();
    first.thinking.as_mut().unwrap().levels[0] = "mutated".to_owned();
    assert_eq!(
        registry.model_info("m1", "gemini").unwrap().display_name,
        "Model One"
    );
    assert_eq!(
        registry.models_for_client("client-1")[0]
            .thinking
            .as_ref()
            .unwrap()
            .levels[0],
        "low"
    );
    assert_eq!(
        registry.available_models_by_provider("gemini")[0]
            .thinking
            .as_ref()
            .unwrap()
            .levels[0],
        "low"
    );
}

struct ManualClock(Mutex<SystemTime>);

impl RegistryClock for ManualClock {
    fn now(&self) -> SystemTime {
        *self.0.lock().unwrap()
    }
}

impl ManualClock {
    fn advance(&self, duration: Duration) {
        let mut now = self.0.lock().unwrap();
        *now += duration;
    }
}

#[test]
fn quota_cleanup_invalidates_cache_and_preserves_registration() {
    let clock = Arc::new(ManualClock(Mutex::new(
        SystemTime::UNIX_EPOCH + Duration::from_secs(1000),
    )));
    let registry =
        ModelRegistry::with_clock(Arc::new(embedded_models_catalog().unwrap()), clock.clone());
    let mut info = model("m1");
    info.created = 1;
    registry.register_client("client-1", "openai", &[info]);
    registry.set_model_quota_exceeded("client-1", "m1");
    assert_eq!(registry.available_models("openai").len(), 1);
    clock.advance(Duration::from_secs(6 * 60));
    registry.cleanup_expired_quotas();
    assert_eq!(registry.model_count("m1"), 1);
    assert_eq!(registry.available_models("openai")[0]["id"], "m1");
}

#[test]
fn available_models_clone_supported_parameters_and_include_max_context_override() {
    let registry = registry();
    registry.register_client(
        "client-1",
        "openai",
        &[RegistryModelInfo {
            id: "deepseek-v4-flash".to_owned(),
            context_length: 1_048_576,
            max_context_length: 1_048_576,
            supported_parameters: vec!["temperature".to_owned(), "top_p".to_owned()],
            ..RegistryModelInfo::default()
        }],
    );
    let mut first = registry.available_models("openai");
    first[0]["supported_parameters"][0] = serde_json::json!("mutated");
    let second = registry.available_models("openai");
    assert_eq!(second[0]["supported_parameters"][0], "temperature");
    assert_eq!(second[0]["context_length"], 1_048_576);
    assert_eq!(second[0]["max_context_length"], 1_048_576);
}

#[test]
fn duplicate_registration_counts_provider_changes_and_reconciliation_match_upstream() {
    let registry = registry();
    registry.register_client("a", "OpenAI", &[model("m1"), model("m1")]);
    registry.register_client("b", "gemini", &[model("m1")]);
    assert_eq!(registry.model_count("m1"), 3);
    assert_eq!(registry.model_providers("m1"), ["openai", "gemini"]);
    registry.register_client("a", "Claude", &[model("m1")]);
    assert_eq!(registry.model_count("m1"), 2);
    assert_eq!(registry.model_providers("m1"), ["claude", "gemini"]);
    registry.register_client("a", "Claude", &[]);
    assert_eq!(registry.model_count("m1"), 1);
    assert_eq!(registry.model_providers("m1"), ["gemini"]);
}

#[test]
fn first_available_model_and_case_insensitive_client_support() {
    let registry = registry();
    let mut old = model("old");
    old.created = 1;
    let mut new = model("new");
    new.created = 2;
    registry.register_client("Client", "openai", &[old, new]);
    assert!(registry.client_supports_model(" Client ", "NEW"));
    assert_eq!(registry.first_available_model("openai").unwrap(), "new");
}
