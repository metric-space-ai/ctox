// ref: sdk/config/config.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Public SDK configuration facade.
//!
//! The pinned Go facade accepts process-relative filenames and lets the
//! internal loader discover additional path authority. CTOX keeps the same
//! type/helper role while requiring callers to provide a typed source and an
//! explicit data root. Mutations similarly use an injected sink.

use std::{collections::BTreeMap, fmt, io, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

pub use crate::internal::config::config_normalization::{OAuthModelAlias, OpenAiCompatibility};
use crate::internal::config::{
    config_load::{load_config as load_internal, ConfigLoadError},
    config_yaml::{
        normalize_comment_indentation as normalize_comment_indentation_internal,
        save_config_preserve_comments as save_internal,
        update_nested_scalar as update_nested_scalar_internal, ConfigYamlError,
    },
    parse::{parse_provider_compat_config_with_root, ProviderCompatConfigError},
    CodexModel, RuntimeSecretRef,
};
pub use crate::internal::config::{
    ClaudeCodeConfig, CodexKey, FileConfigDocument, FileConfigSource, ProviderCompatConfig,
    SdkConfig, StreamingConfig, TypedConfigSink, TypedConfigSource, VertexCompatKey,
    VertexCompatModel,
};

pub type Config = ProviderCompatConfig;
pub type GeminiKey = CodexKey;
pub type XaiKey = CodexKey;
pub type XaiModel = CodexModel;
pub type ClaudeKey = CodexKey;
pub type OpenAiCompatibilityModel = CodexModel;
pub type Tls = TlsConfig;

pub const DEFAULT_PANEL_GITHUB_REPOSITORY: &str =
    crate::internal::config::DEFAULT_PANEL_GITHUB_REPOSITORY;

/// Upstream TLS shape. Paths are inert data until a host explicitly resolves
/// them against its data root.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TlsConfig {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub cert: String,
    #[serde(default)]
    pub key: String,
}

/// Public management shape adapted to reference the CTOX secret store instead
/// of accepting a plaintext management secret.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RemoteManagement {
    #[serde(default)]
    pub allow_remote: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<RuntimeSecretRef>,
    #[serde(default)]
    pub disable_control_panel: bool,
    #[serde(default)]
    pub disable_auto_update_panel: bool,
    #[serde(default)]
    pub panel_github_repository: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PayloadConfig {
    #[serde(default)]
    pub default: Vec<PayloadRule>,
    #[serde(default)]
    pub default_raw: Vec<PayloadRule>,
    #[serde(default)]
    #[serde(rename = "override")]
    pub override_rules: Vec<PayloadRule>,
    #[serde(default)]
    pub override_raw: Vec<PayloadRule>,
    #[serde(default)]
    pub filter: Vec<PayloadFilterRule>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PayloadRule {
    #[serde(default)]
    pub models: Vec<PayloadModelRule>,
    #[serde(default)]
    pub params: BTreeMap<String, JsonValue>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PayloadFilterRule {
    #[serde(default)]
    pub models: Vec<PayloadModelRule>,
    #[serde(default)]
    pub params: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PayloadModelRule {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub from_protocol: String,
    #[serde(default)]
    #[serde(rename = "match")]
    pub matches: Vec<BTreeMap<String, JsonValue>>,
    #[serde(default)]
    #[serde(rename = "not-match")]
    pub not_matches: Vec<BTreeMap<String, JsonValue>>,
    #[serde(default)]
    pub exist: Vec<String>,
    #[serde(default)]
    pub not_exist: Vec<String>,
}

/// OpenAI-compatible credential entry adapted to a secret reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct OpenAiCompatibilityApiKey {
    pub api_key: RuntimeSecretRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<i64>,
    #[serde(default)]
    pub proxy_url: String,
}

pub fn load_config(
    source: &dyn TypedConfigSource,
    data_root: &Path,
) -> Result<Config, ConfigFacadeError> {
    load_internal(source, data_root, false).map_err(ConfigFacadeError::Load)
}

pub fn load_config_optional(
    source: &dyn TypedConfigSource,
    data_root: &Path,
    optional: bool,
) -> Result<Config, ConfigFacadeError> {
    load_internal(source, data_root, optional).map_err(ConfigFacadeError::Load)
}

pub fn parse_config_bytes(data: &[u8], data_root: &Path) -> Result<Config, ConfigFacadeError> {
    parse_provider_compat_config_with_root(data, data_root).map_err(ConfigFacadeError::Parse)
}

pub fn save_config_preserve_comments(
    document: &dyn TypedConfigSink,
    config: &Config,
) -> Result<(), ConfigFacadeError> {
    save_internal(document, config).map_err(ConfigFacadeError::Yaml)
}

pub fn save_config_preserve_comments_update_nested_scalar(
    document: &dyn TypedConfigSink,
    path: &[&str],
    value: &str,
) -> Result<(), ConfigFacadeError> {
    update_nested_scalar_internal(document, path, value).map_err(ConfigFacadeError::Yaml)
}

#[must_use]
pub fn normalize_comment_indentation(data: &[u8]) -> Vec<u8> {
    normalize_comment_indentation_internal(data)
}

#[derive(Debug)]
pub enum ConfigFacadeError {
    Load(ConfigLoadError),
    Parse(ProviderCompatConfigError),
    Read { source: String, error: io::Error },
    Write { source: String, error: io::Error },
    Encode(serde_yaml::Error),
    InvalidPath,
    Yaml(ConfigYamlError),
}

impl fmt::Display for ConfigFacadeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => error.fmt(formatter),
            Self::Parse(error) => error.fmt(formatter),
            Self::Read { source, error } => write!(
                formatter,
                "failed to read config source {source:?}: {error}"
            ),
            Self::Write { source, error } => {
                write!(formatter, "failed to write config sink {source:?}: {error}")
            }
            Self::Encode(error) => write!(formatter, "failed to encode config: {error}"),
            Self::InvalidPath => formatter.write_str("invalid nested configuration path"),
            Self::Yaml(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ConfigFacadeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MemoryDocument(Mutex<Vec<u8>>);
    impl TypedConfigSource for MemoryDocument {
        fn read(&self) -> io::Result<Vec<u8>> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn description(&self) -> String {
            "memory".to_owned()
        }
    }
    impl TypedConfigSink for MemoryDocument {
        fn write(&self, data: &[u8]) -> io::Result<()> {
            self.0.lock().unwrap().clone_from(&data.to_vec());
            Ok(())
        }
    }

    #[test]
    fn facade_loads_and_resolves_only_against_injected_root() {
        let source = MemoryDocument(Mutex::new(b"plugins:\n  dir: ~/extensions\n".to_vec()));
        let config = load_config(&source, Path::new("/srv/ctox")).unwrap();
        assert_eq!(config.plugins.dir, "/srv/ctox/extensions");
    }

    #[test]
    fn optional_and_parse_semantics_delegate_to_internal_core() {
        let empty = MemoryDocument(Mutex::new(Vec::new()));
        assert!(load_config(&empty, Path::new("/data")).is_err());
        assert_eq!(
            load_config_optional(&empty, Path::new("/data"), true)
                .unwrap()
                .plugins
                .dir,
            "plugins"
        );
        assert!(parse_config_bytes(b"unknown: true\n", Path::new("/data")).is_err());
    }

    #[test]
    fn injected_sink_preserves_leading_comments_and_updates_nested_scalars() {
        let document = MemoryDocument(Mutex::new(
            b"# retained\nplugins:\n  enabled: false\n".to_vec(),
        ));
        let config = load_config(&document, Path::new("/data")).unwrap();
        save_config_preserve_comments(&document, &config).unwrap();
        assert!(String::from_utf8(document.read().unwrap())
            .unwrap()
            .starts_with("# retained\n"));
        save_config_preserve_comments_update_nested_scalar(
            &document,
            &["plugins", "enabled"],
            "true",
        )
        .unwrap();
        let value: serde_yaml::Value = serde_yaml::from_slice(&document.read().unwrap()).unwrap();
        assert_eq!(value["plugins"]["enabled"].as_str(), Some("true"));
    }
}
