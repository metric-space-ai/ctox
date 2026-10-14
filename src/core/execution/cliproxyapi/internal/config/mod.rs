// Origin: CTOX
// License: AGPL-3.0-only

// Keep the mirrored upstream `internal/config/config.go` path addressable.
pub mod clone;
pub mod codex_live;
#[allow(clippy::module_inception)]
pub mod config;
mod config_defaults;
pub mod config_load;
pub mod config_normalization;
pub mod config_types;
pub mod config_validation;
pub mod config_yaml;
pub mod credential_concurrency;
pub mod credential_in_flight;
mod disable_image_generation_mode;
mod home;
pub mod parse;
pub mod plugin_path;
pub mod sdk_config;
pub mod vertex_compat;
pub mod weight;

#[cfg(test)]
mod claude_code_test;
#[cfg(test)]
mod claude_header_defaults_test;
#[cfg(test)]
mod clone_test;
#[cfg(test)]
mod codex_live_test;
#[cfg(test)]
mod codex_websocket_header_defaults_test;
#[cfg(test)]
mod credential_concurrency_fixture_test;
#[cfg(test)]
mod credential_concurrency_test;
#[cfg(test)]
mod credential_in_flight_test;
#[cfg(test)]
mod disable_image_generation_mode_test;
#[cfg(test)]
mod home_test;
#[cfg(test)]
mod max_context_length_test;
#[cfg(test)]
mod model_display_name_test;
#[cfg(test)]
mod oauth_model_alias_test;
#[cfg(test)]
mod plugin_config_test;
#[cfg(test)]
mod weight_test;
#[cfg(test)]
mod xai_alpha_search_test;
#[cfg(test)]
mod xai_api_key_test;

pub use codex_live::{CodexLiveIceServer, CodexLiveMediaRelayConfig};
pub use config::*;
pub use config_defaults::{
    DEFAULT_AUTH_DIR, DEFAULT_DISCOVERY_SERVICE_TYPE, DEFAULT_PANEL_GITHUB_REPOSITORY,
    DEFAULT_PPROF_ADDR,
};
pub use config_load::{
    load_config, FileConfigDocument, FileConfigSource, TypedConfigSink, TypedConfigSource,
};
pub use config_normalization::{
    CodexHeaderDefaults, CodexKey, CodexModel, OpenAiCompatibility, OpenAiCompatibilityApiKey,
    ProviderCompatConfig, XaiConfig,
};
pub use credential_concurrency::{
    validate_credential_concurrency, validate_credential_concurrency_lifecycle,
    CredentialConcurrencyConfig,
};
pub use disable_image_generation_mode::{
    DisableImageGenerationMode, DisableImageGenerationModeError,
};
pub use sdk_config::{ClaudeCodeConfig, SdkConfig, StreamingConfig};
pub use vertex_compat::{VertexCompatKey, VertexCompatModel};
pub use weight::{validate_credential_weight, MAX_CREDENTIAL_WEIGHT};
