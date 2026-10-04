// Origin: CTOX
// License: AGPL-3.0-only

pub mod antigravity_models;
pub mod auth;
pub mod builder;
pub mod executionregistry;
pub mod executor;
pub mod home_plugins;
pub mod model_registry;
pub mod pipeline;
pub mod pprof_server;
pub mod providers;
pub mod rtprovider;
pub mod service;
pub mod service_auth;
pub mod service_config;
pub mod service_devin;
pub mod service_executors;
pub mod service_home;
mod service_lifecycle;
pub mod service_meta;
pub mod service_models;
pub mod service_plugins;
pub mod service_runtime;
pub mod session;
pub mod types;
pub mod usage;
pub mod watcher;

#[cfg(test)]
mod service_test_support;

#[cfg(test)]
mod builder_weight_validation_test;
#[cfg(test)]
mod config_model_display_name_test;
#[cfg(test)]
mod config_model_max_context_length_test;
#[cfg(test)]
mod home_plugins_test;
#[cfg(test)]
mod openai_compat_config_models_test;
#[cfg(test)]
mod pprof_server_test;
#[cfg(test)]
mod rtprovider_test;
#[cfg(test)]
mod service_codex_executor_binding_test;
#[cfg(test)]
mod service_codex_models_test;
#[cfg(test)]
mod service_config_weight_test;
#[cfg(test)]
mod service_cooldown_store_test;
#[cfg(test)]
mod service_excluded_models_test;
#[cfg(test)]
mod service_executionregistry_test;
#[cfg(test)]
mod service_executor_registration_test;
#[cfg(test)]
mod service_models_config_index_test;
#[cfg(test)]
mod service_oauth_model_alias_test;
#[cfg(test)]
mod service_plugin_executor_test;
#[cfg(test)]
mod service_plugin_scheduler_test;
#[cfg(test)]
mod service_provider_execution_test;
#[cfg(test)]
mod service_stale_state_test;
#[cfg(test)]
mod types_test;
