// Origin: CTOX
// License: AGPL-3.0-only

pub mod abi;
pub mod adapters;
pub mod adapters_auth;
pub mod adapters_executors;
pub mod adapters_interceptors;
pub mod adapters_quota;
pub mod adapters_usage_translation;
pub mod auth_callbacks;
pub mod auth_provider;
pub mod callback_contexts;
pub mod client_guard;
pub mod command_line;
pub mod config;
pub mod executor_route;
pub mod host;
pub mod host_callbacks;
#[cfg(unix)]
pub mod host_callbacks_unix;
pub mod host_model_stream_callbacks;
pub mod http_bridge;
pub mod http_stream_bridge;
pub mod logging;
pub mod management;
pub mod model_router;
pub mod model_stream_bridge;
pub mod platform;
pub mod process_transport;
pub mod quota_provider;
pub mod rpc_client;
pub mod rpc_client_stream;
pub mod rpc_schema;
pub mod scheduler;
pub mod snapshot;
pub mod stream_bridge;
#[cfg(any(unix, windows))]
#[path = "supervisor_unix.rs"]
pub mod supervisor;
pub mod support;
pub mod support_cgo;
pub mod support_nocgo;
#[cfg(unix)]
pub use supervisor as supervisor_unix;
#[cfg(windows)]
pub use supervisor as supervisor_windows;
#[cfg(unix)]
pub mod transport_unix;
#[cfg(windows)]
pub mod transport_windows;

#[cfg(test)]
mod adapters_quota_test;
#[cfg(test)]
mod adapters_test;
#[cfg(test)]
mod auth_callbacks_test;
#[cfg(test)]
mod auth_provider_test;
#[cfg(test)]
mod client_guard_test;
#[cfg(test)]
mod command_line_test;
#[cfg(test)]
mod config_test;
#[cfg(test)]
mod host_callbacks_test;
#[cfg(test)]
mod host_model_stream_callbacks_test;
#[cfg(test)]
mod host_test;
#[cfg(test)]
mod loader_windows_test;
#[cfg(test)]
mod logging_test;
#[cfg(test)]
mod management_test;
#[cfg(test)]
mod model_router_test;
#[cfg(test)]
mod platform_test;
#[cfg(test)]
mod process_transport_test;
#[cfg(test)]
mod quota_provider_test;
#[cfg(test)]
mod request_lifecycle_test;
#[cfg(test)]
mod rpc_client_error_test;
#[cfg(test)]
mod rpc_client_stream_test;
#[cfg(test)]
mod rpc_schema_test;
#[cfg(test)]
mod scheduler_test;
#[cfg(test)]
mod stream_bridge_test;
#[cfg(test)]
mod test_helpers_test;
#[cfg(all(test, unix))]
mod transport_unix_test;
