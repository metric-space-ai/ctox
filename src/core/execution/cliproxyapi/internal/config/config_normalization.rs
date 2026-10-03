// ref: internal/config/config_normalization.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::internal::registry::RegistryThinkingSupport;

use super::codex_live::CodexLiveMediaRelayConfig;
use super::config_types::RuntimeSecretRef;
use super::plugin_path::{resolve_plugins_dir, DEFAULT_PLUGINS_DIR};
use super::sdk_config::ClaudeCodeConfig;
use super::vertex_compat::{
    normalize_excluded_models, normalize_headers, normalize_model_prefix,
    sanitize_vertex_compat_keys, VertexCompatKey,
};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CodexHeaderDefaults {
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub beta_features: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ClaudeHeaderDefaults {
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub package_version: String,
    #[serde(default)]
    pub runtime_version: String,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub timeout: String,
    #[serde(default)]
    pub timezone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stabilize_device_profile: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct XaiConfig {
    #[serde(default)]
    pub inject_x_search: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CodexProviderConfig {
    #[serde(default)]
    pub identity_confuse: bool,
    #[serde(default)]
    pub disable_codex_cloaking: bool,
    #[serde(default)]
    pub optimize_multi_agent_v2: bool,
    #[serde(default)]
    pub live_media_relay: CodexLiveMediaRelayConfig,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CodexModel {
    pub name: String,
    pub alias: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub max_context_length: usize,
    #[serde(default)]
    pub force_mapping: bool,
    /// Selected configured-model compatibility; never inferred from client input.
    /// ref: internal/config/config_types.go @ 2044a01f
    #[serde(default, skip_serializing_if = "compatibility_disabled")]
    pub is_compat: bool,
    #[serde(default)]
    pub image: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<RegistryThinkingSupport>,
}

fn compatibility_disabled(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CodexKey {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<i64>,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub websockets: bool,
    #[serde(default)]
    pub alpha_search: bool,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub models: Vec<CodexModel>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub excluded_models: Vec<String>,
    #[serde(default)]
    pub disable_cooling: bool,
}

pub type ProviderKey = CodexKey;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct OpenAiCompatibility {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key_entries: Vec<OpenAiCompatibilityApiKey>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub models: Vec<CodexModel>,
    #[serde(default)]
    pub support_prompt_cache_key: bool,
    #[serde(default)]
    pub disable_cooling: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct OpenAiCompatibilityApiKey {
    #[serde(default)]
    pub api_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<i64>,
    #[serde(default)]
    pub proxy_url: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct OAuthModelAlias {
    pub name: String,
    pub alias: String,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub force_mapping: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PluginStoreAuth {
    #[serde(rename = "match")]
    pub match_url: String,
    #[serde(default)]
    pub apply_to: Vec<String>,
    #[serde(rename = "type", default)]
    pub auth_type: String,
    /// Typed CTOX secret-store handle; upstream's `token-env` authority is rejected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_secret: Option<RuntimeSecretRef>,
    #[serde(default)]
    pub header_name: String,
    #[serde(default)]
    pub allow_insecure: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginInstanceConfig {
    pub enabled: Option<bool>,
    pub priority: i32,
    pub raw: serde_yaml::Value,
}

impl Default for PluginInstanceConfig {
    fn default() -> Self {
        Self {
            enabled: Some(false),
            priority: 0,
            raw: serde_yaml::Value::Mapping(Default::default()),
        }
    }
}

impl<'de> Deserialize<'de> for PluginInstanceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = serde_yaml::Value::deserialize(deserializer)?;
        let enabled = raw
            .get("enabled")
            .map(|value| serde_yaml::from_value(value.clone()).map_err(serde::de::Error::custom))
            .transpose()?
            .or(Some(false));
        let priority = raw
            .get("priority")
            .map(|value| serde_yaml::from_value(value.clone()).map_err(serde::de::Error::custom))
            .transpose()?
            .unwrap_or(0);
        Ok(Self {
            enabled,
            priority,
            raw,
        })
    }
}

impl Serialize for PluginInstanceConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.raw.serialize(serializer)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PluginsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub store_sources: Vec<String>,
    #[serde(default)]
    pub store_auth: Vec<PluginStoreAuth>,
    #[serde(default)]
    pub auth_revision: i64,
    #[serde(default)]
    pub configs: BTreeMap<String, PluginInstanceConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProviderCompatConfig {
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub claude_code: ClaudeCodeConfig,
    #[serde(default)]
    pub xai: XaiConfig,
    #[serde(default)]
    pub xai_api_key: Vec<CodexKey>,
    #[serde(default)]
    pub codex_header_defaults: CodexHeaderDefaults,
    #[serde(default)]
    pub claude_header_defaults: ClaudeHeaderDefaults,
    #[serde(default)]
    pub codex: CodexProviderConfig,
    #[serde(default)]
    pub codex_api_key: Vec<CodexKey>,
    #[serde(default)]
    pub claude_api_key: Vec<ProviderKey>,
    #[serde(default)]
    pub gemini_api_key: Vec<ProviderKey>,
    #[serde(default)]
    pub interactions_api_key: Vec<ProviderKey>,
    #[serde(default)]
    pub vertex_api_key: Vec<VertexCompatKey>,
    #[serde(default)]
    pub openai_compatibility: Vec<OpenAiCompatibility>,
    #[serde(default)]
    pub oauth_model_alias: BTreeMap<String, Vec<OAuthModelAlias>>,
    #[serde(default)]
    pub oauth_excluded_models: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "plugins_is_default")]
    pub plugins: PluginsConfig,
}

fn plugins_is_default(plugins: &PluginsConfig) -> bool {
    !plugins.enabled
        && (plugins.dir.is_empty() || plugins.dir == DEFAULT_PLUGINS_DIR)
        && plugins.store_sources.is_empty()
        && plugins.store_auth.is_empty()
        && plugins.auth_revision == 0
        && plugins.configs.is_empty()
}

impl ProviderCompatConfig {
    pub fn sanitize(&mut self) {
        self.codex_header_defaults.user_agent = self.codex_header_defaults.user_agent.trim().into();
        self.codex_header_defaults.beta_features =
            self.codex_header_defaults.beta_features.trim().into();
        for value in [
            &mut self.claude_header_defaults.user_agent,
            &mut self.claude_header_defaults.package_version,
            &mut self.claude_header_defaults.runtime_version,
            &mut self.claude_header_defaults.os,
            &mut self.claude_header_defaults.arch,
            &mut self.claude_header_defaults.timeout,
            &mut self.claude_header_defaults.timezone,
        ] {
            *value = value.trim().into();
        }
        sanitize_xai_keys(&mut self.xai_api_key);
        sanitize_codex_keys(&mut self.codex_api_key);
        sanitize_claude_keys(&mut self.claude_api_key);
        sanitize_gemini_keys(&mut self.gemini_api_key);
        sanitize_gemini_keys(&mut self.interactions_api_key);
        sanitize_vertex_compat_keys(&mut self.vertex_api_key);
        sanitize_openai_compatibility(&mut self.openai_compatibility);
        sanitize_oauth_model_alias(&mut self.oauth_model_alias);
        normalize_oauth_excluded_models(&mut self.oauth_excluded_models);
        normalize_plugins(&mut self.plugins);
    }

    pub fn resolve_plugins_dir(&mut self, data_root: &Path) -> Result<PathBuf, String> {
        let path = resolve_plugins_dir(&self.plugins.dir, data_root)?;
        self.plugins.dir = path.to_string_lossy().into_owned();
        Ok(path)
    }
}

pub fn sanitize_xai_keys(entries: &mut Vec<CodexKey>) {
    sanitize_codex_keys(entries);
    for entry in entries {
        entry.alpha_search = false;
    }
}

fn sanitize_codex_keys(entries: &mut Vec<ProviderKey>) {
    entries.retain_mut(|entry| {
        entry.prefix = normalize_model_prefix(&entry.prefix);
        entry.base_url = entry.base_url.trim().into();
        normalize_headers(&mut entry.headers);
        normalize_excluded_models(&mut entry.excluded_models);
        !entry.base_url.is_empty()
    });
}

fn sanitize_claude_keys(entries: &mut [ProviderKey]) {
    for entry in entries {
        entry.prefix = normalize_model_prefix(&entry.prefix);
        normalize_headers(&mut entry.headers);
        normalize_excluded_models(&mut entry.excluded_models);
    }
}

fn sanitize_gemini_keys(entries: &mut Vec<ProviderKey>) {
    let mut seen = HashSet::new();
    entries.retain_mut(|entry| {
        entry.api_key = entry.api_key.trim().into();
        entry.prefix = normalize_model_prefix(&entry.prefix);
        entry.base_url = entry.base_url.trim().into();
        entry.proxy_url = entry.proxy_url.trim().into();
        normalize_headers(&mut entry.headers);
        normalize_excluded_models(&mut entry.excluded_models);
        !entry.api_key.is_empty() && seen.insert(format!("{}|{}", entry.api_key, entry.base_url))
    });
}

fn sanitize_openai_compatibility(entries: &mut Vec<OpenAiCompatibility>) {
    entries.retain_mut(|entry| {
        entry.name = entry.name.trim().into();
        entry.prefix = normalize_model_prefix(&entry.prefix);
        entry.base_url = entry.base_url.trim().into();
        normalize_headers(&mut entry.headers);
        !entry.base_url.is_empty()
    });
}

pub fn sanitize_oauth_model_alias(entries: &mut BTreeMap<String, Vec<OAuthModelAlias>>) {
    let mut out = BTreeMap::new();
    for (channel, aliases) in std::mem::take(entries) {
        let channel = channel.trim().to_ascii_lowercase();
        let mut seen = HashSet::new();
        let aliases = aliases
            .into_iter()
            .filter_map(|mut entry| {
                entry.name = entry.name.trim().into();
                entry.alias = entry.alias.trim().into();
                entry.display_name = entry.display_name.trim().into();
                let valid = !entry.name.is_empty()
                    && !entry.alias.is_empty()
                    && !entry.name.eq_ignore_ascii_case(&entry.alias)
                    && seen.insert(entry.alias.to_ascii_lowercase());
                valid.then_some(entry)
            })
            .collect::<Vec<_>>();
        if !channel.is_empty() && !aliases.is_empty() {
            out.insert(channel, aliases);
        }
    }
    *entries = out;
}

fn normalize_oauth_excluded_models(entries: &mut BTreeMap<String, Vec<String>>) {
    let mut out = BTreeMap::new();
    for (provider, mut models) in std::mem::take(entries) {
        let provider = provider.trim().to_ascii_lowercase();
        normalize_excluded_models(&mut models);
        if !provider.is_empty() && !models.is_empty() {
            out.insert(provider, models);
        }
    }
    *entries = out;
}

fn normalize_plugins(plugins: &mut PluginsConfig) {
    plugins.dir = plugins.dir.trim().into();
    if plugins.dir.is_empty() {
        plugins.dir = DEFAULT_PLUGINS_DIR.into();
    }
    plugins.store_sources = std::mem::take(&mut plugins.store_sources)
        .into_iter()
        .map(|source| source.trim().to_owned())
        .filter(|source| !source.is_empty())
        .collect();
    plugins.store_auth = std::mem::take(&mut plugins.store_auth)
        .into_iter()
        .filter_map(|mut auth| {
            auth.match_url = auth.match_url.trim().into();
            auth.auth_type = auth.auth_type.trim().to_ascii_lowercase();
            auth.header_name = auth.header_name.trim().into();
            let mut seen = HashSet::new();
            auth.apply_to = auth
                .apply_to
                .into_iter()
                .map(|kind| kind.trim().to_ascii_lowercase())
                .filter(|kind| !kind.is_empty() && seen.insert(kind.clone()))
                .collect();
            (!auth.match_url.is_empty()).then_some(auth)
        })
        .collect();
}

impl CodexHeaderDefaults {
    #[must_use]
    pub fn websocket_defaults(
        &self,
        disable_cloaking: bool,
    ) -> crate::internal::runtime::executor::CodexWebsocketHeaderDefaults {
        crate::internal::runtime::executor::CodexWebsocketHeaderDefaults {
            user_agent: non_empty(&self.user_agent),
            beta: non_empty(&self.beta_features),
            disable_cloaking,
            ..crate::internal::runtime::executor::CodexWebsocketHeaderDefaults::default()
        }
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}
