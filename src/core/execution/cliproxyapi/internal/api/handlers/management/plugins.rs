// ref: internal/api/handlers/management/plugins.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: typed public configuration over injected durable and runtime authorities
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;

use crate::internal::htmlsanitize;
use crate::sdk::pluginapi::{ConfigField, Metadata};

const MAX_PLUGIN_ID_LENGTH: usize = 96;
const MAX_CONFIG_FIELDS: usize = 128;

#[derive(Clone, Default, Eq, PartialEq)]
pub struct ManagementPluginConfig {
    pub enabled: bool,
    pub priority: i32,
    pub values: BTreeMap<String, Value>,
}

impl fmt::Debug for ManagementPluginConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagementPluginConfig")
            .field("enabled", &self.enabled)
            .field("priority", &self.priority)
            .field("values", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct ManagementPluginConfigPatch {
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub values: BTreeMap<String, Option<Value>>,
}

impl fmt::Debug for ManagementPluginConfigPatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagementPluginConfigPatch")
            .field("enabled", &self.enabled)
            .field("priority", &self.priority)
            .field("values", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ManagementPluginSnapshot {
    pub revision: u64,
    pub plugins_enabled: bool,
    pub configs: BTreeMap<String, ManagementPluginConfig>,
}

pub trait ManagementPluginConfigStore: Send + Sync {
    fn load(&self) -> Result<ManagementPluginSnapshot, ManagementPluginConfigStoreError>;
    fn replace(
        &self,
        expected_revision: u64,
        snapshot: &ManagementPluginSnapshot,
    ) -> Result<(), ManagementPluginConfigStoreError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagementPluginConfigStoreError {
    Unavailable,
    Conflict,
}

// ref: internal/api/handlers/management/plugins.go:44-61,465-500
// @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManagementPluginConfigField {
    #[serde(rename = "default")]
    pub default_value: Option<Value>,
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub enum_values: Vec<String>,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManagementPluginMetadata {
    pub name: String,
    pub version: String,
    pub author: String,
    pub github_repository: String,
    pub logo: String,
    pub config_fields: Vec<ManagementPluginConfigField>,
}

fn plugin_config_fields(fields: &[ConfigField]) -> Vec<ManagementPluginConfigField> {
    fields
        .iter()
        .map(|field| ManagementPluginConfigField {
            default_value: field.default_value.as_ref().map(htmlsanitize::json_value),
            name: htmlsanitize::string(&field.name),
            field_type: htmlsanitize::string(&field.field_type.0),
            enum_values: htmlsanitize::strings(&field.enum_values),
            description: htmlsanitize::string(&field.description),
        })
        .collect()
}

pub(super) fn plugin_metadata(metadata: &Metadata) -> ManagementPluginMetadata {
    ManagementPluginMetadata {
        name: htmlsanitize::string(&metadata.name),
        version: htmlsanitize::string(&metadata.version),
        author: htmlsanitize::string(&metadata.author),
        github_repository: htmlsanitize::string(&metadata.github_repository),
        logo: htmlsanitize::string(&metadata.logo),
        config_fields: plugin_config_fields(&metadata.config_fields),
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ManagementPluginRuntimeRecord {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_ref: Option<String>,
    pub registered: bool,
    pub supports_oauth: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    // Only the sanitized projection reaches management responses; this is
    // declared registration schema, never configured account values.
    #[serde(skip_serializing)]
    pub metadata: Option<Metadata>,
}

pub trait ManagementPluginRuntimeSource: Send + Sync {
    fn snapshot(&self) -> Result<Vec<ManagementPluginRuntimeRecord>, ManagementPluginError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManagementPluginView {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_ref: Option<String>,
    pub configured: bool,
    pub registered: bool,
    pub enabled: bool,
    pub effective_enabled: bool,
    pub supports_oauth: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    pub logo: String,
    pub config_fields: Vec<ManagementPluginConfigField>,
    pub metadata: Option<ManagementPluginMetadata>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManagementPluginError {
    StoreUnavailable,
    Conflict,
    RuntimeUnavailable,
    InvalidPluginId,
    InvalidConfig,
    NotFound,
}

impl fmt::Display for ManagementPluginError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StoreUnavailable => "plugin configuration store unavailable",
            Self::Conflict => "plugin configuration changed concurrently",
            Self::RuntimeUnavailable => "plugin runtime unavailable",
            Self::InvalidPluginId => "plugin identifier is invalid",
            Self::InvalidConfig => "plugin configuration is invalid",
            Self::NotFound => "plugin not found",
        })
    }
}

impl std::error::Error for ManagementPluginError {}

pub struct ManagementPluginService {
    pub(super) store: Arc<dyn ManagementPluginConfigStore>,
    runtime: Arc<dyn ManagementPluginRuntimeSource>,
    pub(super) mutation: Mutex<()>,
}

impl ManagementPluginService {
    #[must_use]
    pub fn new(
        store: Arc<dyn ManagementPluginConfigStore>,
        runtime: Arc<dyn ManagementPluginRuntimeSource>,
    ) -> Self {
        Self {
            store,
            runtime,
            mutation: Mutex::new(()),
        }
    }

    pub fn list(&self) -> Result<Vec<ManagementPluginView>, ManagementPluginError> {
        let snapshot = self.load()?;
        let runtime = self.runtime.snapshot()?;
        for record in &runtime {
            validate_plugin_id(&record.id)?;
            if record
                .install_ref
                .as_deref()
                .is_some_and(|value| !valid_install_ref(value))
                || record
                    .oauth_provider
                    .as_deref()
                    .is_some_and(|value| validate_plugin_id(value).is_err())
                || record
                    .capabilities
                    .iter()
                    .any(|value| value.len() > 64 || validate_plugin_id(value).is_err())
            {
                return Err(ManagementPluginError::RuntimeUnavailable);
            }
        }
        let mut ids = snapshot.configs.keys().cloned().collect::<BTreeSet<_>>();
        ids.extend(runtime.iter().map(|record| record.id.clone()));
        let by_id = runtime
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect::<BTreeMap<_, _>>();
        Ok(ids
            .into_iter()
            .map(|id| {
                let config = snapshot.configs.get(&id);
                let runtime = by_id.get(&id);
                let registered = runtime.is_some_and(|record| record.registered);
                let enabled = config.is_some_and(|config| config.enabled);
                let metadata = runtime
                    .filter(|record| record.registered)
                    .and_then(|record| record.metadata.as_ref())
                    .map(plugin_metadata);
                ManagementPluginView {
                    id,
                    install_ref: runtime.and_then(|record| record.install_ref.clone()),
                    configured: config.is_some(),
                    registered,
                    enabled,
                    effective_enabled: snapshot.plugins_enabled && enabled && registered,
                    supports_oauth: runtime.is_some_and(|record| record.supports_oauth),
                    oauth_provider: runtime.and_then(|record| record.oauth_provider.clone()),
                    capabilities: runtime
                        .map_or_else(Vec::new, |record| record.capabilities.clone()),
                    logo: metadata
                        .as_ref()
                        .map_or_else(String::new, |meta| meta.logo.clone()),
                    config_fields: metadata
                        .as_ref()
                        .map_or_else(Vec::new, |meta| meta.config_fields.clone()),
                    metadata,
                }
            })
            .collect())
    }

    pub fn public_config(&self, id: &str) -> Result<ManagementPluginConfig, ManagementPluginError> {
        let id = validate_plugin_id(id)?;
        self.load()?
            .configs
            .get(id)
            .cloned()
            .ok_or(ManagementPluginError::NotFound)
    }

    pub fn put_config(
        &self,
        id: &str,
        config: ManagementPluginConfig,
    ) -> Result<ManagementPluginConfig, ManagementPluginError> {
        validate_config(&config)?;
        self.mutate(id, move |configs, id| {
            configs.insert(id.to_owned(), config.clone());
            Ok(config)
        })
    }

    pub fn patch_config(
        &self,
        id: &str,
        patch: ManagementPluginConfigPatch,
    ) -> Result<ManagementPluginConfig, ManagementPluginError> {
        validate_patch(&patch)?;
        self.mutate(id, move |configs, id| {
            let config = configs.get_mut(id).ok_or(ManagementPluginError::NotFound)?;
            if let Some(enabled) = patch.enabled {
                config.enabled = enabled;
            }
            if let Some(priority) = patch.priority {
                config.priority = priority;
            }
            for (key, value) in patch.values {
                if let Some(value) = value {
                    config.values.insert(key, value);
                } else {
                    config.values.remove(&key);
                }
            }
            validate_config(config)?;
            Ok(config.clone())
        })
    }

    pub fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<ManagementPluginConfig, ManagementPluginError> {
        self.mutate(id, move |configs, id| {
            let config = configs.entry(id.to_owned()).or_default();
            config.enabled = enabled;
            Ok(config.clone())
        })
    }

    pub fn delete_config(&self, id: &str) -> Result<ManagementPluginConfig, ManagementPluginError> {
        self.mutate(id, |configs, id| {
            configs.remove(id).ok_or(ManagementPluginError::NotFound)
        })
    }

    pub(super) fn lock_mutation(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn load(&self) -> Result<ManagementPluginSnapshot, ManagementPluginError> {
        let snapshot = self.store.load().map_err(store_error)?;
        if snapshot
            .configs
            .iter()
            .any(|(id, config)| validate_plugin_id(id).is_err() || validate_config(config).is_err())
        {
            return Err(ManagementPluginError::InvalidConfig);
        }
        Ok(snapshot)
    }

    pub(super) fn replace(
        &self,
        expected_revision: u64,
        snapshot: &ManagementPluginSnapshot,
    ) -> Result<(), ManagementPluginError> {
        self.store
            .replace(expected_revision, snapshot)
            .map_err(store_error)
    }

    fn mutate<T>(
        &self,
        id: &str,
        mutate: impl FnOnce(
            &mut BTreeMap<String, ManagementPluginConfig>,
            &str,
        ) -> Result<T, ManagementPluginError>,
    ) -> Result<T, ManagementPluginError> {
        let id = validate_plugin_id(id)?;
        let _guard = self.lock_mutation();
        let mut snapshot = self.load()?;
        let expected_revision = snapshot.revision;
        let result = mutate(&mut snapshot.configs, id)?;
        snapshot.revision = expected_revision.saturating_add(1);
        self.replace(expected_revision, &snapshot)?;
        Ok(result)
    }
}

impl fmt::Debug for ManagementPluginService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagementPluginService")
            .finish_non_exhaustive()
    }
}

pub fn validate_plugin_id(id: &str) -> Result<&str, ManagementPluginError> {
    let id = id.trim();
    if id.is_empty()
        || id.len() > MAX_PLUGIN_ID_LENGTH
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        Err(ManagementPluginError::InvalidPluginId)
    } else {
        Ok(id)
    }
}

fn validate_config(config: &ManagementPluginConfig) -> Result<(), ManagementPluginError> {
    if config.values.len() > MAX_CONFIG_FIELDS
        || config
            .values
            .iter()
            .any(|(key, value)| !valid_public_key(key) || contains_sensitive_key(value))
    {
        return Err(ManagementPluginError::InvalidConfig);
    }
    Ok(())
}

fn validate_patch(patch: &ManagementPluginConfigPatch) -> Result<(), ManagementPluginError> {
    if patch.values.len() > MAX_CONFIG_FIELDS
        || patch.values.iter().any(|(key, value)| {
            !valid_public_key(key) || value.as_ref().is_some_and(contains_sensitive_key)
        })
    {
        return Err(ManagementPluginError::InvalidConfig);
    }
    Ok(())
}

fn valid_public_key(key: &str) -> bool {
    let key = key.trim();
    !key.is_empty()
        && key.len() <= 96
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        && !sensitive_key(key)
}

fn contains_sensitive_key(value: &Value) -> bool {
    match value {
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| sensitive_key(key) || contains_sensitive_key(value)),
        Value::Array(values) => values.iter().any(contains_sensitive_key),
        _ => false,
    }
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace('-', "_");
    key == "token"
        || key.ends_with("_token")
        || key.contains("secret")
        || key == "password"
        || key.ends_with("_password")
        || key == "api_key"
        || key.ends_with("_api_key")
}

fn valid_install_ref(value: &str) -> bool {
    value.starts_with("plugin:")
        && value.len() <= 192
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'@' | b'.' | b'-' | b'_')
        })
}

fn store_error(error: ManagementPluginConfigStoreError) -> ManagementPluginError {
    match error {
        ManagementPluginConfigStoreError::Unavailable => ManagementPluginError::StoreUnavailable,
        ManagementPluginConfigStoreError::Conflict => ManagementPluginError::Conflict,
    }
}
