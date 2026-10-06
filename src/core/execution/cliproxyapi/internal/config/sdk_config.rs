// ref: internal/config/sdk_config.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde::{Deserialize, Serialize};

use super::DisableImageGenerationMode;

/// Provider-neutral server settings from the public SDK configuration.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SdkConfig {
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub disable_image_generation: DisableImageGenerationMode,
    #[serde(default)]
    pub gpt_image_2_base_model: String,
    #[serde(default)]
    pub video_result_auth_cache_ttl: String,
    #[serde(default)]
    pub force_model_prefix: bool,
    #[serde(default)]
    pub request_log: bool,
    /// Runtime-only mirror used by handlers. YAML uses `client.codex.optimize-multi-agent-v2`.
    #[serde(skip)]
    pub codex_optimize_multi_agent_v2: bool,
    /// v8 provider settings that must wait for credential selection and must not
    /// affect API-key credentials. Not serialized.
    #[serde(skip)]
    pub oauth_only_fields: std::collections::BTreeMap<String, bool>,
    /// Provider-wide runtime setting for API handlers. Not serialized.
    #[serde(skip)]
    pub codex_response_steering: bool,
    /// Provider-wide runtime setting for API handlers. Not serialized.
    #[serde(skip)]
    pub codex_orphan_delegation_compatibility: bool,
    #[serde(default)]
    pub client: ClientConfig,
    #[serde(default)]
    pub claude_code: ClaudeCodeConfig,
    #[serde(default)]
    pub api_keys: Vec<String>,
    #[serde(default)]
    pub passthrough_headers: bool,
    #[serde(default)]
    pub streaming: StreamingConfig,
    #[serde(default)]
    pub nonstream_keepalive_interval: i32,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ClientConfig {
    #[serde(default)]
    pub codex: CodexClientConfig,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CodexClientConfig {
    /// Optimizes official Codex multi-agent requests across providers.
    /// Default false leaves the client's multi-agent behavior unchanged.
    #[serde(default)]
    pub optimize_multi_agent_v2: bool,
    /// Advertises freeform apply_patch only for supported models.
    /// Default false clears the capability regardless of template metadata.
    #[serde(default)]
    pub enable_apply_patch: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ClaudeCodeConfig {
    #[serde(default)]
    pub disable_cloaking_model_list: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StreamingConfig {
    /// SSE heartbeat interval, or WebSocket ping interval. `<= 0` disables it.
    #[serde(default)]
    pub keepalive_seconds: i32,
    #[serde(default)]
    pub bootstrap_retries: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_surface_is_closed_and_preserves_streaming_values() {
        let config: SdkConfig = serde_yaml::from_str(
            "passthrough-headers: true\nstreaming:\n  keepalive-seconds: 5\n  bootstrap-retries: 2\n",
        )
        .unwrap();
        assert!(config.passthrough_headers);
        assert_eq!(config.streaming.keepalive_seconds, 5);
        assert_eq!(config.streaming.bootstrap_retries, 2);
        assert!(serde_yaml::from_str::<SdkConfig>("unknown: true\n").is_err());
    }

    #[test]
    fn client_codex_compatibility_is_explicit_yaml() {
        let config: SdkConfig = serde_yaml::from_str(
            "client:\n  codex:\n    optimize-multi-agent-v2: true\n    enable-apply-patch: true\n",
        )
        .unwrap();
        assert!(config.client.codex.optimize_multi_agent_v2);
        assert!(config.client.codex.enable_apply_patch);
        assert!(!config.codex_optimize_multi_agent_v2);
        assert!(config.oauth_only_fields.is_empty());
    }
}
