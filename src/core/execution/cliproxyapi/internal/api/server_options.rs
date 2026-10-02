// ref: internal/api/server_options.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::internal::config::{ProviderCompatConfig, SdkConfig};

pub trait ServerReloadHook: Send + Sync {
    fn config_reloaded(&self, revision: u64);
}

pub trait ServerTimeoutHook: Send + Sync {
    fn timed_out(&self);
}

/// Typed, transport-neutral server assembly options. Middleware and router
/// callbacks remain owned by the host transport instead of leaking a Gin-like
/// framework into the Rust core.
#[derive(Clone, Default)]
pub struct ServerOptions {
    pub local_management_enabled: bool,
    pub keepalive_timeout: Option<Duration>,
    pub example_api_key_safe_mode: bool,
    pub reload_hook: Option<Arc<dyn ServerReloadHook>>,
    pub timeout_hook: Option<Arc<dyn ServerTimeoutHook>>,
}

impl fmt::Debug for ServerOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerOptions")
            .field("local_management_enabled", &self.local_management_enabled)
            .field("keepalive_timeout", &self.keepalive_timeout)
            .field("example_api_key_safe_mode", &self.example_api_key_safe_mode)
            .field("has_reload_hook", &self.reload_hook.is_some())
            .field("has_timeout_hook", &self.timeout_hook.is_some())
            .finish()
    }
}

#[must_use]
pub fn effective_sdk_config(sdk: &SdkConfig, providers: &ProviderCompatConfig) -> SdkConfig {
    let mut effective = sdk.clone();
    effective.codex_optimize_multi_agent_v2 = providers.codex.optimize_multi_agent_v2
        || sdk.client.codex.optimize_multi_agent_v2;
    effective
}
