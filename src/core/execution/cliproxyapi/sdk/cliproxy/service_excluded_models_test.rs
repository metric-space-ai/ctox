// ref: sdk/cliproxy/service_excluded_models_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::antigravity_models::AntigravityModelCapabilityCatalog;
use super::auth::Auth;
use super::service_models::{
    apply_excluded_models, ConfiguredModel, OpenAiCompatibilityConfig, ServiceModelConfig,
    ServiceModelRuntime,
};
use crate::internal::registry::{
    embedded_models_catalog, ModelRegistry as InternalRegistry, RegistryModelInfo,
    OPENAI_IMAGE_MODEL_TYPE,
};
use std::collections::BTreeMap;
use std::sync::Arc;

#[test]
fn wildcard_exclusions_are_case_insensitive_and_order_preserving() {
    let models = ["gemini-pro", "GEMINI-flash", "claude"]
        .into_iter()
        .map(|id| RegistryModelInfo {
            id: id.into(),
            ..RegistryModelInfo::default()
        })
        .collect();
    let filtered = apply_excluded_models(models, &["gemini-*".into()]);
    assert_eq!(
        filtered
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["claude"]
    );
}

#[test]
fn premerged_auth_exclusions_override_global_provider_exclusions() {
    let catalog = Arc::new(embedded_models_catalog().unwrap());
    let registry = Arc::new(InternalRegistry::new(Arc::clone(&catalog)));
    let facade: Arc<dyn super::model_registry::ModelRegistry> = registry.clone();
    let service = ServiceModelRuntime::new(
        ServiceModelConfig {
            oauth_excluded_models: BTreeMap::from([(
                "gemini".into(),
                vec!["gemini-2.5-pro".into()],
            )]),
            ..ServiceModelConfig::default()
        },
        facade,
        catalog,
    );
    let mut auth = Auth::default();
    auth.id = "gemini-auth".into();
    auth.provider = "gemini".into();
    auth.attributes = BTreeMap::from([
        ("auth_kind".into(), "oauth".into()),
        ("excluded_models".into(), "gemini-2.5-flash".into()),
    ]);
    service.register_models_for_auth(&auth);
    assert!(!registry.client_supports_model("gemini-auth", "gemini-2.5-flash"));
    assert!(registry.client_supports_model("gemini-auth", "gemini-2.5-pro"));
}

#[test]
fn antigravity_discovery_annotates_static_model_without_registering_fetched_only_model() {
    let catalog = AntigravityModelCapabilityCatalog::new();
    catalog
        .replace_from_response(
            [
                "gemini-3.1-flash-lite",
                "gemini-3-flash-agent",
                "gpt-oss-120b-medium",
            ],
            br#"{
                "models": {
                    "gemini-3.1-flash-lite": {"maxTokens": 1},
                    "fetched-only-search-model": {"displayName": "Fetched"}
                },
                "webSearchModelIds": [
                    "gemini-3.1-flash-lite",
                    "fetched-only-search-model"
                ]
            }"#,
        )
        .unwrap();

    assert!(catalog.supports_web_search("gemini-3.1-flash-lite"));
    assert!(!catalog.supports_web_search("gemini-3-flash-agent"));
    assert!(!catalog.supports_web_search("gpt-oss-120b-medium"));
    assert!(!catalog.supports_web_search("fetched-only-search-model"));
}
