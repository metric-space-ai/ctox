// Origin: CTOX
// License: AGPL-3.0-only

mod codex_client_models;
mod codex_client_models_updater;
mod devin_builtin;
mod devin_models;
mod model_definitions;
mod model_registry;
mod model_updater;

pub use codex_client_models::{
    validate_codex_client_models_json, CodexClientModelsError, CodexClientModelsSnapshot,
    CodexClientModelsStore,
};
pub use codex_client_models_updater::{
    default_codex_client_models_sources, refresh_codex_client_models, CatalogFetchFailure,
    CatalogRefresh, CatalogRefreshError, CodexClientModelsFetchFuture, CodexClientModelsSource,
    DEFAULT_CODEX_CLIENT_MODELS_SOURCES, MAX_CODEX_CLIENT_MODELS_SIZE,
};
#[cfg(feature = "codex-http-transport")]
pub use codex_client_models_updater::{WreqCatalogSourceBuildError, WreqCodexClientModelsSource};
pub use devin_models::{
    validate_devin_models_json, DevinModelsError, DevinModelsSnapshot, DevinModelsStore,
    EMBEDDED_DEVIN_MODELS_JSON,
};
pub use model_definitions::{
    antigravity_web_search_model_for, embedded_models_catalog, lookup_model_info,
    lookup_static_registry_model_info, model_override_headers, models_for_channel,
    parse_models_catalog, static_model_definitions_by_channel, validate_models_catalog,
    with_codex_builtins, with_devin_builtins, with_xai_builtins, ModelConfig, ModelInfo,
    NativeCapabilities, RegistryModelInfo, RegistryThinkingSupport, StaticModelCatalogError,
    StaticModelsCatalog, ThinkingSupport,
};
pub use model_registry::{
    HookContext, ModelRegistry, ModelRegistryHook, RegistryClock, RegistryError,
    SystemRegistryClock, DEFAULT_CLAUDE_MAX_INPUT_TOKENS, DEFAULT_CLAUDE_MAX_OUTPUT_TOKENS,
    DEFAULT_MODEL_REGISTRY_HOOK_TIMEOUT, MODEL_QUOTA_EXCEEDED_WINDOW, OPENAI_IMAGE_MODEL_TYPE,
};
pub use model_updater::{
    detect_changed_providers, merge_provider_names, CatalogLoad, ModelCatalogSnapshot,
    ModelCatalogStore, ModelFetchFailure, ModelRefreshCallback, ModelRefreshSink,
    ModelsFetchFuture, ModelsRefresh, ModelsRefreshError, ModelsSource, ModelsUpdater,
    DEFAULT_DEVIN_MODELS_URLS, DEFAULT_MODELS_URLS, MAX_DEVIN_MODELS_CATALOG_SIZE,
    MAX_MODELS_CATALOG_SIZE, MODELS_FETCH_TIMEOUT, MODELS_REFRESH_INTERVAL,
};
#[cfg(feature = "codex-http-transport")]
pub use model_updater::{WreqModelsSource, WreqModelsSourceBuildError};

#[cfg(test)]
mod codex_client_models_test;
#[cfg(test)]
mod devin_models_test;
#[cfg(test)]
mod model_catalog_v8_test;
#[cfg(test)]
mod model_definitions_test;
#[cfg(test)]
mod model_registry_cache_test;
#[cfg(test)]
mod model_registry_hook_test;
#[cfg(test)]
mod model_registry_safety_test;
